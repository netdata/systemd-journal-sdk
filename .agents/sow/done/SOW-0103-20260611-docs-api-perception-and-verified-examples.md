# SOW-0103 - Docs API Perception Restructure And Verified Examples

## Status

Status: completed

Sub-state: delivered, validated, reviewed production-grade by all five pool
reviewers in round 3, audited clean, closed with the work in one commit.

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
  SOW-0104, SOW-0105, SOW-0106; closed SOW-0065 as superseded. Committed as
  the planning chunk (`4a6f7b50`).
- Chunk 1 implementer run started: `llm-netdata-cloud/minimax-m3-coder` via
  `opencode run` with the prompt at `.local/sow-0103/implementer-run-a.md`
  (harness `tests/docs/verify_examples.py`, unit tests, testdata sample page).
  Cache redirection env exported per orchestration skill.
- Chunk 2 implementer prompt prepared at
  `.local/sow-0103/implementer-run-b.md` (validator extension plus
  `.github/workflows/docs-examples.yml`); runs after chunk 1 completes.
- Project-manager docs restructure (non-marker part) applied:
  - new `docs/Journalctl-CLI.md` page documenting the four file-backed
    journalctl rewrites as the fifth consumption surface;
  - `docs/API-Overview.md`: layer map now includes the journalctl CLI; new
    `Choosing An API Surface` decision-path section; verified-examples trust
    statement; journalctl row in reader surfaces;
  - `docs/Home.md`, `docs/Getting-Started.md`, `docs/Reader-APIs.md`,
    `docs/_Sidebar.md`: navigation and selection-table entries for the CLI
    page plus the trust statement;
  - `docs/Wiki-Publishing.md`: documents the marker convention, placeholder
    path vocabulary, and the local `verify_examples.py` command.
  - `python3 tests/docs/check_wiki_docs.py` green on 15 pages after edits.
- Chunk 1 implementer run completed (exit 0): delivered
  `tests/docs/verify_examples.py`, `tests/docs/test_verify_examples.py`
  (31 unit tests green), `tests/docs/testdata/Sample-Page.md`. End-to-end
  testdata pass verified by the project manager: 5/5. Extraction is
  fence-aware (marker examples inside ```markdown fences in
  `Wiki-Publishing.md` are ignored as content).
- Project manager converted `docs/Rust-API.md` and `docs/Go-API.md`:
  31 marked examples (17 Go, 14 Rust) plus illustrative-only markers on
  dependency/install/import fragments. Snippet repairs for standalone
  compilation: Go raw-append/write-binary use `if err := ...; err != nil`
  with the `open-writer` prelude, Go field-name-policy got error check and
  close, Go source-selector got explicit `_ = function`, fragments now state
  the prelude they continue from.
- First real-docs verification: 17/17 Go examples PASS; 14/14 Rust examples
  fail on one systematic harness bug: the Rust wrapper appends `Ok(())` after
  a body already ending in turbofish `Ok::<(), Box<dyn std::error::Error>>(())`.
  All documented Rust API names were verified present in `rust/` sources
  (explorer.rs:1203 explore, log/config.rs builder methods, netdata.rs:311
  from_timeout_seconds, lib.rs:27 verify_file re-export), so the docs are
  accurate and the failure is harness-side only.
- Implementer fix run A2 completed: Ok-ending detection now treats turbofish
  `Ok::<...>(...)` endings as terminal; unit tests extended. Project-manager
  verification after A2: unit tests OK and the full real-docs run passes
  31 of 31 examples (17 Go, 14 Rust), all against synthetic fixtures under
  `.local/docs-examples/`.
- Chunk 2 implementer run launched with
  `.local/sow-0103/implementer-run-b.md` (fence-aware validator marker
  enforcement plus `.github/workflows/docs-examples.yml`). Note recorded:
  the prompt predicted validator failure on unconverted API pages, but the
  pages were converted before the run, so a passing validator is the
  expected outcome now.
- Added runtime project skill
  `.agents/skills/project-docs-authoring/SKILL.md` (marker grammar,
  placeholder vocabulary, validation commands, perception model) and
  registered it in `AGENTS.md`.
- Project-manager line-by-line review of chunk-1 deliverables (the
  implementer's tests alone are not accepted as sufficient evidence):
  - Verified clean: fence-aware extraction, longest-first substitution
    order, self-contained caches under `.local/docs-examples/caches`,
    synthetic fixture builder with `verify_file` sanity check, correct
    exit codes, timeout handling, no host-journal access, no writes
    outside `.local/`.
  - FINDING H1 (must fix): example id uniqueness is enforced per page only
    (`extract_examples` resets `seen_ids` per file,
    `tests/docs/verify_examples.py:228`); a cross-page duplicate id would
    silently overwrite the generated binary. Global uniqueness required.
  - FINDING H2 (minor): extraction `HarnessError` propagates as a raw
    traceback instead of a clean `die()` message.
  - FINDING H3 (cosmetic): unused imports `uuid`/`Iterable`, dead
    `build_records` parameter in `_print_summary`, and a duplicated test
    method name `test_run_mode_with_question_mark_wraps_result`
    (`tests/docs/test_verify_examples.py:202,211`, second shadows first).
  - NOTE for chunk-2 verification: the harness intentionally pins its own
    caches under `.local/docs-examples/caches`; the CI workflow cache steps
    must target those paths, not `~/.cargo`/`~/go`.
  Findings go to the implementer as one batch fix run (A3) after chunk 2.
- Chunk 2 run completed partially: it delivered the fence-aware validator
  extension in `tests/docs/check_wiki_docs.py` (grammar imported from the
  harness module so validator and harness cannot drift; global id
  uniqueness across files; marker-fence pairing checks) plus 26 unit tests
  in `tests/docs/test_check_wiki_docs.py`, but ended before writing the CI
  workflow. Project-manager verification caught the missing deliverable
  (the run exited 0; its log ends mid-task), and the validator diff review
  found no defects.
- The extended validator found three unmarked rust/go fences the initial
  conversion missed (`docs/Getting-Started.md:27`,
  `docs/Getting-Started.md:43`, `docs/Rust-Crates-And-Packages.md:14`);
  the project manager marked all three illustrative-only (import
  fragments). Extended validator now passes on real docs; its unit tests
  pass.
- Combined fix run B2 launched with
  `.local/sow-0103/implementer-run-b2.md`: the missing
  `.github/workflows/docs-examples.yml` (caching the harness-pinned
  `.local/docs-examples/caches` paths) plus harness findings H1, H2, H3.
- B2 run completed and verified line-by-line by the project manager:
  - `.github/workflows/docs-examples.yml` created; pinned checkout and Rust
    toolchain match existing workflows, `actions/setup-go@v6` uses
    `go-version-file`, cache targets the harness-pinned
    `.local/docs-examples/caches` keyed on `rust/Cargo.lock` + `go/go.sum`
    (both verified present), `permissions: contents: read`, correct
    triggers and concurrency.
  - H1 fixed: `discover_examples` shares one `seen_ids` map across pages;
    duplicate-id error names the first-seen page:line.
  - H2 fixed: `main()` catches `HarnessError` and exits via `die()`.
  - H3 fixed: unused imports and dead parameter removed; shadowed test
    renamed (`test_run_mode_with_explicit_result_return_appends_ok`).
- Final local validation by the project manager (2026-06-11):
  - `python3 tests/docs/test_verify_examples.py` OK;
  - `python3 tests/docs/test_check_wiki_docs.py` OK;
  - `python3 tests/docs/check_wiki_docs.py` green on 15 pages with marker
    enforcement active;
  - `python3 tests/docs/verify_examples.py --docs-dir tests/docs/testdata`
    5/5;
  - `python3 tests/docs/verify_examples.py` 31/31 (17 Go, 14 Rust);
  - negative case: cross-page duplicate id produces a clean one-line error
    with first-seen location and exit 1, no traceback;
  - `git status` confirms no modifications under `rust/`, `go/`,
    `python/`, `node/`.
- Whole-SOW reviewer batch launched: five `llm-netdata-cloud` reviewers in
  parallel, read-only (`--agent code-reviewer`), prompt at
  `.local/sow-0103/reviewer-prompt.md`.
- Reviewer round 1 results (whole-SOW scope, read-only, parallel; raw
  reports under `.local/sow-0103/review-*.txt`):
  - `glm-5.1`: PRODUCTION GRADE: YES. Non-blocking: suggested a unit test
    for the substitution longest-prefix-first invariant (accepted, fix
    batch); `serde_json = "1"` semver pin (accepted as standard Cargo
    practice); Go `return nil` after terminal returns (explicitly allowed
    by spec).
  - `mimo-v2.5-pro`: PRODUCTION GRADE: YES. Non-blocking: hardcoded
    `go 1.26` in the generated module (accepted, fix batch reads the
    directive from `go/go.mod`); substring import detection and `?`
    detection heuristics (accepted as documented limitations).
  - `qwen3.7-plus`: PRODUCTION GRADE: YES. Only note: actions pinning
    consistency, classified by the reviewer itself as pre-existing repo
    convention and non-blocking.
  - `deepseek-v4-pro`: PRODUCTION GRADE: NO. Blocking finding VALIDATED by
    the project manager against
    `rust/src/journal/src/sealed_verify.rs:83-99`: `docs/Journalctl-CLI.md`
    described `--verify-key <hex>` while the actual format is
    `<seed>/<start>-<interval>`. Fixed by the project manager in the page
    prose. Its pinning recommendation is handled with kimi's finding.
  - `kimi-k2.6`: PRODUCTION GRADE: NO. Blocking: unpinned
    `actions/setup-go@v6` and `actions/cache@v4` in the new workflow.
    Disposition: although coverage.yml uses identical floating tags (repo
    convention), pinning costs nothing and matches wiki.yml's strictest
    pattern; both actions are pinned to upstream tag SHAs in the fix batch
    (new workflow only; repo-wide pinning policy is out of scope). Its
    MEDIUM parallel-run race on the shared `.local/docs-examples/` work
    root is accepted as a documented single-instance limitation; the
    claimed reproduction message does not exist in the harness, but the
    race itself is real in principle.
- Fix batch C launched with `.local/sow-0103/implementer-run-c.md`:
  pin two actions, read the `go` directive from `go/go.mod`, document the
  heuristic/single-instance limitations, add the substitution-invariant
  unit test. The verify-key prose fix was applied directly by the project
  manager.
- Fix batch C verified by the project manager in code and behavior: both
  actions pinned to upstream tag SHAs with version comments,
  `_read_go_directive()` parses `go/go.mod` with error paths plus 4 tests,
  Known-limitations docstring section added, longest-prefix invariant test
  added. Full suite green again (31/31, 5/5 testdata, both unit suites,
  validator 15 pages).
- Reviewer round 2 (identical whole-SOW scope plus fix notes; reports under
  `.local/sow-0103/review2-*.txt`): glm YES, mimo YES, qwen YES, deepseek
  YES (round-1 blocker confirmed fixed; flagged stray scratch binaries in
  the repo root left by fix run C — verified as today's implementer
  artifacts and deleted), kimi NO with two NEW validated findings:
  - K1: `_build_rust`/`_build_go` do not catch `subprocess.TimeoutExpired`
    (only `_run_example` does, `verify_examples.py:792`); build timeout
    would traceback.
  - K2: generated example package hardcodes `edition = "2021"` while the
    workspace uses `edition = "2024"` (`rust/Cargo.toml:39`); examples must
    compile under the workspace edition.
- Fix run D launched with `.local/sow-0103/implementer-run-d.md` (K1 +
  K2 with tests; prompt now also forbids scratch compilation outside
  `.local/` after the stray-binaries incident).
- Fix run D verified by the project manager: `TimeoutExpired` caught in
  all three subprocess sites with stub tests for both build paths;
  `_read_rust_edition()` reads `[workspace.package] edition` from
  `rust/Cargo.toml` with five tests (match, error paths, section scoping).
  Run D again left stray scratch binaries in the repo root despite the
  prompt prohibition; removed, and post-run root cleanup added to the
  project-manager verification routine for this implementer.
- Reviewer round 3 (identical scope plus cumulative fix notes; reports
  under `.local/sow-0103/review3-*.txt`): glm YES, mimo YES, qwen YES,
  deepseek YES, kimi first run cut off mid-review with no verdict
  (recorded; retried once), kimi retry YES. Final round verdict: 5/5
  PRODUCTION GRADE.
- `.agents/sow/audit.sh`: one CRITICAL sensitive-data hit was a false
  positive (the rule parsed the ledger wording starting with the word
  PASS plus a colon as a password assignment); reworded the ledger line,
  audit reruns clean.

## Validation

Acceptance criteria evidence:

- Marker convention documented in `docs/Wiki-Publishing.md`; markers are
  HTML comments, invisible in rendered wiki output; validator enforces
  them on every rust/go fence.
- Harness `tests/docs/verify_examples.py` extracts, compiles against the
  local Rust workspace (path deps, workspace edition) and Go module
  (replace directive, go directive from `go/go.mod`), and runs examples
  against synthetic fixtures under `.local/docs-examples/`; failures exit
  nonzero with a manifest.
- Every code block in `docs/Rust-API.md` and `docs/Go-API.md` is verified
  (31 examples: 17 Go, 14 Rust) or explicitly illustrative-only
  (dependency/install/import fragments with reasons). All other pages'
  rust/go fences are marked; sh/toml/markdown fences are exempt by rule.
- `docs/API-Overview.md` restructured around the API-perception decision
  paths; the journalctl rewrite CLI is documented as the fifth consumption
  surface in the new `docs/Journalctl-CLI.md`.
- CI: `.github/workflows/docs-examples.yml` runs both unit suites, the
  validator, the testdata pass, and the real-docs pass on docs/harness/
  rust/go/python changes; wiki publishing workflow untouched.
- Rust and Go source trees unmodified (verified by `git status` repeatedly
  and by all five reviewers).

Tests or equivalent validation:

- `python3 tests/docs/test_verify_examples.py` green (grew from 31 to 45+
  tests across fix rounds).
- `python3 tests/docs/test_check_wiki_docs.py` green (26 tests).
- `python3 tests/docs/check_wiki_docs.py` green on 15 pages with marker
  enforcement.
- `python3 tests/docs/verify_examples.py --docs-dir tests/docs/testdata`
  5/5; `python3 tests/docs/verify_examples.py` 31/31.
- Negative cases verified by the project manager: cross-page duplicate id
  fails with a clean one-line error and exit 1.

Real-use evidence:

- The harness executed all 31 documented examples end-to-end against
  synthetic journal fixtures, exercising the real public Rust and Go APIs
  exactly as a consumer would compile them.

Reviewer findings:

- Round 1: glm YES, mimo YES, qwen YES, deepseek NO, kimi NO. Validated
  blockers fixed: verify-key format wording in `docs/Journalctl-CLI.md`
  (deepseek; confirmed against `rust/src/journal/src/sealed_verify.rs`),
  action SHA pinning in the new workflow (kimi). Non-blocking accepted
  improvements: go directive from `go/go.mod`, substitution-invariant
  test, documented heuristic/single-instance limitations. Rejected with
  reason: repo-wide action-pinning policy change (out of scope; new
  workflow pinned), kimi's claimed race reproduction message (does not
  exist in the harness; underlying single-instance limitation documented).
- Round 2: glm, mimo, qwen, deepseek YES; kimi NO with two new validated
  findings (build-phase TimeoutExpired, workspace edition drift), fixed in
  run D; deepseek's stray-binaries housekeeping fixed.
- Round 3: 5/5 PRODUCTION GRADE: YES (kimi after one recorded mid-review
  cutoff and retry).

Same-failure scan:

- Unmarked-fence class: validator now enforces repo-wide; the three
  initially missed fences (Getting-Started, Rust-Crates-And-Packages) were
  found by the new check itself and marked.
- Hardcoded-toolchain class: after the go-directive finding, the Rust
  edition twin was found in round 2 and both now read from the canonical
  manifests.
- Stray scratch binaries: repo root re-checked after every implementer
  run; clean at close.

Sensitive data gate:

- Durable artifacts contain no secrets, credentials, tokens, personal
  data, or private endpoints; fixtures and examples use synthetic
  identities only. The single audit hit was a false positive on ledger
  wording and was reworded; audit reruns clean.

Artifact maintenance gate:

- AGENTS.md: updated (routing decision, reviewer pool correction, skill
  registration, Rust/Go freeze note).
- Runtime project skills: orchestration skill routing updated; new
  `.agents/skills/project-docs-authoring/SKILL.md` added.
- Specs: no product behavior change; the docs-verification contract is
  recorded in the docs-authoring skill and `docs/Wiki-Publishing.md`, so
  no `.agents/sow/specs/` update is needed.
- End-user/operator docs: this SOW is the docs change (15 wiki pages).
- End-user/operator skills: none exist for this surface; not affected.
- SOW lifecycle: closed as completed and moved to `done/` in the same
  commit as the work; SOW-0065 was superseded at program start.
- SOW-status.md: both the canonical ledger and the root convenience index
  updated at close.

Specs update:

- No spec update needed: no product/SDK behavior changed; the verified
  -examples workflow is HOW-knowledge, captured in the project skill and
  the operator-facing `docs/Wiki-Publishing.md`.

Project skills update:

- `.agents/skills/project-docs-authoring/SKILL.md` added;
  `.agents/skills/project-agent-orchestration/SKILL.md` routing updated.

End-user/operator docs update:

- `docs/`: API-Overview, Home, Getting-Started, Reader-APIs,
  Wiki-Publishing, Rust-API, Go-API, Rust-Crates-And-Packages, _Sidebar
  updated; Journalctl-CLI added.

End-user/operator skills update:

- None exist for this surface; nothing to update.

Lessons:

- Implementer self-tests are insufficient evidence: the turbofish Ok bug
  passed the implementer's own testdata; only the real-docs run caught it.
- Exit code 0 from an external agent does not mean the scope was
  delivered: run B silently dropped the workflow deliverable.
- Iterating reviewers with full scope pays: kimi's round-2 findings (build
  timeout, edition drift) were new, real, and invisible in round 1.
- This implementer compiles scratch tests in the working directory even
  when forbidden; post-run repo-root cleanup is a standing verification
  step.
- Audit regexes can false-positive on ledger prose (the word PASS followed
  by a colon); prefer wording like "Verified clean:" in SOW ledgers.

Follow-up mapping:

- Tracked: SOW-0104 (Python parity), SOW-0105 (Node parity), SOW-0106
  (Python/Node docs; also extends the harness to python/js examples and
  may add sh-example verification for the Journalctl-CLI page, whose
  command blocks are illustrative-only in this SOW).
- No other deferred items: the SOW's "defer/later/future/TODO/pending"
  scan at close maps every item to the entries above or to completed
  work.

## Outcome

Delivered. The consumer wiki is restructured around how developers should
perceive the API surfaces (decision paths, five consumption surfaces
including the previously undocumented journalctl CLI), and every Rust and
Go example in the wiki is now a machine-verified contract: 31 examples
compile against the local workspace/module and run against synthetic
fixtures, enforced locally and in CI, with marker discipline guaranteed by
the wiki validator. Five reviewer models returned PRODUCTION GRADE: YES in
the final round.

## Lessons Extracted

See Validation - Lessons. The durable operational ones (verification
routine for delegated work, docs-authoring rules) are captured in the
orchestration and docs-authoring project skills.

## Followup

- SOW-0104, SOW-0105, SOW-0106 as mapped above; SOW-0104 activates next.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and
later testing or use found broken behavior. Use a dated `## Regression -
YYYY-MM-DD` heading at the end of the file. Never prepend regression content
above the original SOW narrative.
