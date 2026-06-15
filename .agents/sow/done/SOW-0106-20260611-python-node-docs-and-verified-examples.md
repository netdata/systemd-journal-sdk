# SOW-0106 - Python And Node.js Docs With Verified Examples

## Status

Status: completed

Sub-state: completed on 2026-06-15 after local validation, two reviewer
batches, reviewer-finding fixes, and clean SOW audit.

## Requirements

### Purpose

Extend the end-user wiki documentation to Python and Node.js after they reach
Rust parity, with every code example verified by the SOW-0103 harness, so all
four languages have equal, perception-focused, machine-verified developer
documentation.

### User Request

2026-06-11: improve end-user developer documentation with verified examples,
focused on how developers should perceive the various APIs, across the SDK
program. This SOW carries the Python/Node share, deferred behind parity by the
user's program-order decision.

### Assistant Understanding

Facts (verified 2026-06-11):

- `docs/` has no `Python-API.md` or `Node-API.md`; `Reader-APIs.md` and
  `Writer-APIs.md` selection tables cover Rust and Go only;
  `Explorer-And-Netdata-Queries.md` covers Rust and Go only;
  `Getting-Started.md` install guidance is thin for Python/Node.
- SOW-0103 delivers the marker convention, extraction harness, fixtures, and
  CI verification for Rust and Go examples; this SOW extends harness language
  support to Python and Node.
- SOW-0104 adds `pyproject.toml`, enabling a real local `pip install -e`
  install path in docs; SOW-0105 adds `.d.ts`, enabling typed Node examples.

Inferences:

- Python/Node examples should run against the same synthetic fixtures the
  Rust/Go examples use, keeping one fixture corpus.
- Production-Profiles performance guidance (Rust/Go for production
  throughput; Python/Node as compatibility surfaces) remains true and must be
  restated, not weakened, in the new pages.

Unknowns:

- Final shape of the Python/Node Explorer/Netdata APIs until SOW-0104/0105
  close; example content is written against the shipped surfaces.

### Acceptance Criteria

- `Python-API.md` and `Node-API.md` wiki pages exist, mirroring the
  Rust-API/Go-API structure (reader, payload visitor, directory, snapshot,
  unique values, Explorer, Netdata function, writer structured/raw), with all
  examples verified by the harness.
- The harness supports Python and Node example execution; CI verifies all
  four languages' examples.
- `Reader-APIs.md`, `Writer-APIs.md`, `Explorer-And-Netdata-Queries.md`,
  `Getting-Started.md`, `Options-Reference.md`, and `_Sidebar.md` gain
  Python/Node columns/entries; `Production-Profiles.md` guidance stays honest.
- `tests/docs/check_wiki_docs.py` passes; wiki publishing works.
- Rust and Go sources unmodified.
- Whole-SOW reviewer batches return production-grade.

## Analysis

Sources checked:

- 2026-06-11 docs inventory (this program's planning session): per-page
  assessment of all 14 wiki pages, validator, and `wiki.yml` workflow.
- SOW-0100 for wiki structure and publishing lessons.

Current state:

- Wiki is Rust/Go-centric; Python/Node covered only by per-language READMEs.

Risks:

- Writing docs before parity ships would document a moving target; prevented
  by program order.
- Example runtime cost in CI grows with two more languages; mitigate with
  fixture reuse and per-language job splitting if needed.

## Pre-Implementation Gate

Status: ready for implementation

Activation evidence:

- SOW-0104, SOW-0105, SOW-0107, SOW-0109, and SOW-0111 are completed.
- `python/README.md` and `node/README.md` now describe the shipped Python and
  Node.js reader, writer, Explorer, Netdata, access-mode, and platform
  contracts.
- `node/index.d.ts` exists and documents the default-package public TypeScript
  surface.
- `tests/docs/verify_examples.py` currently supports only Rust and Go, so
  Python and Node.js verified-example support is still the missing harness
  work.

Problem / root-cause model:

- Wiki docs cover only the two production languages even though Python and
  Node.js now have parity surfaces. Missing Python/Node pages and verified
  examples leave users with unequal developer documentation and leave future
  docs examples untested.

Evidence reviewed:

- Docs inventory listed in Analysis.
- Current `docs/` pages: no `Python-API.md` or `Node-API.md`; shared reader,
  writer, Explorer/Netdata, options, getting-started, production-profile, and
  sidebar pages remain Rust/Go-focused.
- Current `tests/docs/verify_examples.py`: `SUPPORTED_LANGS = ("rust", "go")`.
- Current `tests/docs/check_wiki_docs.py`: marker validation depends on the
  harness language list.
- Current Python and Node READMEs and Node TypeScript declarations.

Affected contracts and surfaces:

- `docs/*.md` (new and updated pages), `tests/docs/verify_examples.py`
  language support, CI workflow, `_Sidebar.md` navigation.

Existing patterns to reuse:

- SOW-0103 marker convention, harness, fixtures, and page structure;
  Rust-API/Go-API page layout as the template.
- Existing placeholder path substitution for repository-local synthetic
  fixtures and per-example scratch paths.

Risk and blast radius:

- Docs and test-infrastructure only; no production code paths.

Sensitive data handling plan:

- Synthetic identities and repository-local fixtures only.

Implementation plan:

1. Extend harness language support to Python and Node.
2. Write `Python-API.md` and `Node-API.md` (project manager prose, verified
   examples iterated to green).
3. Update shared pages and navigation.
4. Validation, reviewer batches, audit, close.

Validation plan:

- `python3 tests/docs/check_wiki_docs.py`.
- `python3 tests/docs/verify_examples.py`, proving all Rust, Go, Python, and
  Node.js verified examples compile/run against synthetic fixtures.
- Relevant Python/Node package checks if harness changes require them.
- Reviewer batches against the whole SOW after local validation.
- `git diff --check`.
- `.agents/sow/audit.sh`.

Artifact impact plan:

- AGENTS.md: no change expected.
- Runtime project skills: update `project-docs-authoring` if marker grammar,
  verified-example language support, placeholders, validation commands, or CI
  workflow contract changes.
- Specs: no product behavior change expected.
- End-user/operator docs: this SOW is the docs change.
- End-user/operator skills: no output/reference skill expected; verify at
  close.
- SOW lifecycle: activate in `.agents/sow/current/`; close only after
  validation and reviewer gates pass.
- SOW-status.md: update canonical and root ledgers on activation and close.

Open-source reference evidence:

- None checked at creation.

Open decisions:

- None; user decisions recorded in SOW-0103 apply.

## Implications And Decisions

1. 2026-06-11 program-order and routing decisions recorded in SOW-0103 apply.

## Plan

1. Harness extension.
2. New language pages.
3. Shared-page updates.
4. Reviews, audit, close.

## Delegation Plan

Implementer:

- `llm-netdata-cloud/minimax-m3-coder` via
  `timeout 1800 opencode run -m "llm-netdata-cloud/minimax-m3-coder" "<prompt>"`
  for harness/CI code; docs prose by the project manager.

Reviewers:

- `llm-netdata-cloud/glm-5.1`, `llm-netdata-cloud/kimi-k2.6`,
  `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.7-plus`,
  `llm-netdata-cloud/deepseek-v4-pro`, read-only, whole-SOW batches.

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

- As recorded in SOW-0103.

## Execution Log

### 2026-06-11

- Created as pending final child of the docs-and-parity program; activates
  after SOW-0105.

### 2026-06-15

- Activated after Python/Node parity closure and SOW-0111 reader API parity
  closure.
- Refreshed the pre-implementation gate against current docs, Python README,
  Node README, TypeScript declarations, and the verified-example harness.
- Attempted one external implementer run for the harness extension. The shell
  prompt contained Markdown backticks, which were interpreted before launch and
  mangled the prompt; the run was stopped by killing only the exact timeout and
  child PIDs for this SOW. The worktree was clean afterward, so no implementer
  changes were accepted from that run.
- Implemented the verified-examples harness extension locally:
  - `tests/docs/verify_examples.py` now supports `rust`, `go`, `python`, and
    `javascript` verified examples;
  - Python examples are syntax-checked with `py_compile` and executed with
    `PYTHONPATH=python`;
  - JavaScript examples are syntax-checked with `node --check`, executed with
    Node.js, and package imports from `@netdata/systemd-journal-sdk` are
    rewritten in generated sources to the local Node.js source entry point;
  - all generated sources, scratch data, caches, and manifests remain under
    `.local/docs-examples/`.
- Added Python and JavaScript testdata examples so
  `tests/docs/verify_examples.py --docs-dir tests/docs/testdata` exercises all
  four languages.
- Updated the wiki authoring docs, project docs-authoring skill, validator
  comments/tests, and docs-examples workflow to cover Python and JavaScript
  verified examples. CI now watches `node/**` and sets up Node 26.
- Added `docs/Python-API.md` and `docs/Node-API.md`, mirroring the
  Rust/Go page structure with verified examples for readers, payload
  visitors, row DATA enumeration, directory reads, snapshot bounds, unique
  values, Explorer, direct and directory writers, field-name policy, optional
  writer locks, Netdata function boundaries, custom source selectors, and
  verification.
- Updated shared wiki pages and navigation for Python/Node coverage:
  `Home.md`, `Getting-Started.md`, `API-Overview.md`, `Reader-APIs.md`,
  `Writer-APIs.md`, `Explorer-And-Netdata-Queries.md`,
  `Options-Reference.md`, `Production-Profiles.md`,
  `Hot-Path-Guide.md`, and `_Sidebar.md`.
- While writing verified Node Netdata examples, found that `node/README.md`
  and `node/index.d.ts` documented Netdata config/profile classes that were
  already implemented but not exported by `node/src/index.js`. Exported those
  public classes from the package entry point so the verified docs can import
  the documented API.
- Corrected `node/index.d.ts` for `FileReader.queryUnique()` and
  `DirectoryReader.queryUnique()` from `string[]` to `Bytes[]`, matching the
  runtime implementation and the documented unique-value examples.
- Ran the first whole-SOW external reviewer batch after local validation.
  GLM, Kimi, Qwen, and DeepSeek voted production-grade; Mimo did not return a
  usable verdict before the local wrapper failed while collecting logs, so Mimo
  is being rerun with the second reviewer batch.
- Addressed actionable reviewer findings:
  - added `WriterLock.path` to `node/index.d.ts`, matching the runtime
    property used by the verified writer-lock example;
  - exported the already-declared seal, FSS, and output-format helper symbols
    from `node/src/index.js`, aligning runtime named exports with the existing
    TypeScript declarations;
  - adjusted `docs/Options-Reference.md` to describe Python/Node seal options
    as writer-surface options instead of implying identical class exports.
- Ran the second whole-SOW reviewer batch after those fixes. GLM, Kimi, Mimo,
  Qwen, and DeepSeek all voted production-grade with no blocking findings.
- Addressed two valid non-blocking second-batch findings:
  - corrected `node/index.d.ts` so `SealOptions`, `SealState`, and
    `WriterOptions.seal` match the actual Node.js runtime seal API;
  - corrected `docs/Writer-APIs.md` so the FSS quick-selection row does not
    imply Python exposes a top-level `SealOptions` import.

## Validation

Acceptance criteria evidence:

- Harness language support for Python and JavaScript is implemented and
  locally validated.
- `docs/Python-API.md` and `docs/Node-API.md` exist with 17 verified examples
  each.
- Shared wiki pages and `_Sidebar.md` include Python/Node entries and keep
  the production-performance guidance honest: Rust/Go remain the
  high-throughput production targets; Python/Node remain compatibility and
  integration surfaces unless workload-specific benchmarks prove otherwise.
- Node package entry-point exports now match the Netdata classes documented by
  `node/README.md` and `node/index.d.ts`; `queryUnique()` TypeScript return
  types match the runtime `Bytes[]` return values.

Tests or equivalent validation:

- Harness chunk validation passed:
  - `python3 tests/docs/test_verify_examples.py`: 56/56 passed.
  - `python3 tests/docs/test_check_wiki_docs.py`: 27/27 passed.
  - `python3 tests/docs/check_wiki_docs.py`: validated 15 wiki markdown files.
  - `python3 tests/docs/verify_examples.py --docs-dir tests/docs/testdata`:
    passed 7/7, covering Rust, Go, Python, and JavaScript.
  - `python3 tests/docs/verify_examples.py`: passed 31/31 current wiki
    examples.
  - `git diff --check`: passed.
- Python/Node docs chunk validation passed:
  - `python3 tests/docs/test_check_wiki_docs.py`: 27/27 passed.
  - `python3 tests/docs/test_verify_examples.py`: 56/56 passed.
  - `python3 tests/docs/check_wiki_docs.py`: validated 17 wiki markdown files.
  - `python3 tests/docs/verify_examples.py --docs-dir tests/docs/testdata`:
    passed 7/7, covering Rust, Go, Python, and JavaScript.
  - `python3 tests/docs/verify_examples.py`: passed 65/65 current wiki
    examples: 14 Rust, 17 Go, 17 Python, and 17 JavaScript examples.
  - `npm run typecheck` in `node/`: passed.
  - Direct ES module import from `node/src/index.js` confirmed exported
    `NetdataFunctionConfig`, `NetdataFunctionRunOptions`,
    `SystemdJournalProfile`, and `WriterLock`.
  - `npm test` in `node/`: passed.
  - `git diff --check`: passed.
  - `bash .agents/sow/audit.sh`: passed.
- Post-review fix validation passed:
  - `python3 tests/docs/verify_examples.py`: passed 65/65 current wiki
    examples: 14 Rust, 17 Go, 17 Python, and 17 JavaScript examples.
  - `python3 tests/docs/check_wiki_docs.py`: validated 17 wiki markdown files.
  - `npm run typecheck` in `node/`: passed.
  - Direct ES module import from `node/src/index.js` confirmed
    `SealOptions`, `SealState`, `exportEntry`, `exportEntryBuffer`,
    `jsonEntry`, `textEntry`, `fsprgGenMK`, `fsprgGenState0`,
    `fsprgEvolve`, `fsprgSeek`, `fsprgGetKey`, `fsprgGetEpoch`, and
    `WriterLock` resolve as runtime functions.
  - `npm test` in `node/`: passed.
  - `git diff --check`: passed.
- Second-batch finding validation passed:
  - `python3 tests/docs/verify_examples.py`: passed 65/65 current wiki
    examples: 14 Rust, 17 Go, 17 Python, and 17 JavaScript examples.
  - `python3 tests/docs/check_wiki_docs.py`: validated 17 wiki markdown files.
  - `npm run typecheck` in `node/`: passed.
  - Direct ES module construction confirmed `new SealOptions(Buffer.alloc(12),
    1000000n, 0n)` and `new SealState(opts)` work through the package entry
    point, with HMAC output length 32.
  - `npm test` in `node/`: passed.
  - `git diff --check`: passed.

Real-use evidence:

- Testdata verification and full wiki verification executed generated Python
  and JavaScript examples against the same synthetic fixture corpus used by
  Rust and Go examples. No live host journal was probed.

Reviewer findings:

- First whole-SOW review batch:
  - DeepSeek: production-grade; found `WriterLock.path` was used by docs but
    absent from `node/index.d.ts`. Fixed in `node/index.d.ts`.
  - GLM: production-grade; found a pre-existing named export gap where
    TypeScript declarations exposed seal, FSS, and output-format helper
    symbols that `node/src/index.js` did not export. Fixed the named runtime
    exports and included those symbols in the existing default export object.
    GLM also noted close-section placeholders, which are expected until SOW
    closure.
  - Kimi: production-grade; noted Python writer examples could use context
    managers. Disposition: no code/doc change; the current explicit
    `try/finally` examples are verified and make close behavior obvious.
  - Qwen: production-grade; noted the Node default export object is broader in
    runtime than the `.d.ts` contract and not fully parallel to named exports.
    Disposition: no additional change in this SOW; docs use named imports and
    `index.d.ts` does not declare a default-export contract. Qwen also noted
    Python `WriterLock` is not top-level; docs already import it from
    `journal.lock`, so no issue exists.
  - Mimo: no usable verdict in the first batch; rerun required in the second
    batch.
- Second whole-SOW review batch:
  - GLM: production-grade; noted the Node default export has no `.d.ts`
    default-export declaration, `Bytes` maps to `Uint8Array` for TypeScript
    ergonomics, and close-section placeholders were still pending. Disposition:
    no code/doc change for default export or `Bytes`; docs use named imports,
    no default export contract exists, and `Bytes` is a pre-existing ergonomic
    tradeoff outside this docs SOW.
  - Kimi: production-grade; independently confirmed the stale `SealOptions`
    and `SealState` constructor declarations. Fixed in `node/index.d.ts`.
    Kimi also noted `FileReader.path` nullability and default-export
    inconsistency as pre-existing/out-of-scope.
  - Mimo: production-grade; noted `WriterLock` is absent from the runtime
    default export object and Python writer examples use `try/finally` instead
    of context managers. Disposition: no code/doc change; docs use named
    imports and the explicit close examples are verified and intentional.
  - Qwen: production-grade; noted the runtime default export object does not
    include every named export. Disposition: no code/doc change for the same
    reason as above.
  - DeepSeek: production-grade; found the `SealOptions`/`SealState`
    TypeScript constructor mismatch and the `Writer-APIs.md` Python FSS cell
    wording issue. Fixed both.

Same-failure scan:

- Searched changed harness/docs for stale Rust/Go-only wording and corrected
  the relevant validator, harness, wiki-publishing, and project-skill text.

Sensitive data gate:

- This SOW contains no raw sensitive data.

Artifact maintenance gate:

- `AGENTS.md`: no change. The project-wide workflow, responsibilities, and
  repository guardrails did not change.
- Runtime project skills: updated `.agents/skills/project-docs-authoring`
  because the verified-example marker grammar and language support changed to
  include Python and JavaScript.
- Specs: no `.agents/sow/specs/` update. Existing specs cover product scope,
  Rust reader performance, systemd-journal plugin facets, and Codacy metrics;
  this SOW changed consumer docs and docs-harness mechanics, not product
  behavior that belongs in those specs.
- End-user/operator docs: updated by this SOW, including new Python/Node pages
  and shared wiki pages.
- End-user/operator skills: no output/reference skills exist in this
  repository, so no update was applicable.
- SOW lifecycle: status set to `completed`; file will move from
  `.agents/sow/current/` to `.agents/sow/done/` during close.
- SOW status ledgers: canonical `.agents/sow/SOW-status.md` and root
  `SOW-status.md` updated during close.

Specs update:

- No spec update was needed. The durable WHAT contracts affected here are the
  end-user wiki pages themselves and the Node TypeScript declaration surface;
  there is no matching product spec for wiki/example coverage.

Project skills update:

- Updated `.agents/skills/project-docs-authoring/SKILL.md` because the
  verified-example marker grammar and language support changed.

End-user/operator docs update:

- Updated `docs/Wiki-Publishing.md` for the expanded verified-example language
  set.
- Added `docs/Python-API.md` and `docs/Node-API.md`.
- Updated shared wiki pages and navigation listed in the execution log.

End-user/operator skills update:

- No end-user/operator output skill exists in this repository. The runtime
  authoring skill was updated under Project skills.

Lessons:

- Quote-heavy shell prompts are unsafe for delegated runs unless passed via a
  prompt file or equivalent argument-safe path; the failed implementer attempt
  validated that this should remain the default for long prompts.
- Verified examples are a useful compatibility forcing function: they exposed
  real Node package entry-point/type gaps that ordinary prose review could
  have missed.
- A second reviewer batch after fixing first-round issues is valuable, but
  small production-grade follow-up fixes can be closed with focused local
  validation instead of restarting the full pool again.

Follow-up mapping:

- No new follow-up SOW is required. The only remaining reviewer observations
  are explicit no-change dispositions: Node default-export completeness has no
  declared TypeScript contract and docs use named imports; Python writer
  examples intentionally use explicit `try/finally`; `Bytes` ergonomics and
  `FileReader.path` nullability are pre-existing type ergonomics outside this
  docs SOW and not exposed by the verified examples as incorrect behavior.

## Outcome

Completed. Python and Node.js now have first-class wiki API pages with 17
verified examples each, shared wiki pages cover all four languages, the docs
harness verifies Rust, Go, Python, and JavaScript examples against synthetic
fixtures, Node entry-point/type gaps found by the verified docs and reviewers
were corrected, and two whole-SOW reviewer batches returned production-grade
with no blocking findings.

## Lessons Extracted

- Use `.local/*.txt` prompt files for long external-agent prompts that contain
  Markdown, shell-sensitive characters, or fenced code.
- Treat docs examples as public API tests: when an example needs an import,
  the package entry point and type declarations must be checked, not only the
  implementation module.
- Keep Python/Node production guidance explicit. They are verified parity and
  integration surfaces here, while Rust/Go remain the high-throughput
  production targets unless workload benchmarks prove otherwise.

## Followup

None.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and
later testing or use found broken behavior. Use a dated `## Regression -
YYYY-MM-DD` heading at the end of the file. Never prepend regression content
above the original SOW narrative.
