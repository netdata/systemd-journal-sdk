# SOW-0106 - Python And Node.js Docs With Verified Examples

## Status

Status: in-progress

Sub-state: activated on 2026-06-15 after Python/Node parity closure and
SOW-0111 reader API parity closure.

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

- Pending implementation.

Project skills update:

- Pending implementation.

End-user/operator docs update:

- Pending implementation.

End-user/operator skills update:

- Pending implementation.

Lessons:

- Pending implementation.

Follow-up mapping:

- Pending implementation.

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
