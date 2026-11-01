# SOW-0153 - Consumer Documentation Release Clarity

## Status

Status: completed

Sub-state: completed 2026-11-01 in the autonomous backlog run; decision 1A is
agent-chosen, pending user review (PR body).

## Requirements

### Purpose

Keep consumer documentation consistent with the published release guidance and
make the existing Rust writer-state APIs discoverable alongside Go's error API.

### User Request

The user requires Rust/Go parity. Project close-out rules require valid deferred
review findings to have a real pending SOW; this record does not expand the
authorized publication work into SDK or documentation implementation.

### Assistant Understanding

Facts:

- `go/API.md:9-10` still identifies `go/v0.3.0` as the expected consumable tag.
  This wording predates PR #6; `go/README.md` contains the current installation
  instructions for the prepared paired `0.9.0` release.
- `docs/Writer-APIs.md:60-72` explains uncertain writer failures and names Go's
  `ErrWriterFailed`, but does not name the existing Rust `is_poisoned()` methods.
- Rust provides `JournalWriter::is_poisoned()` at
  `rust/src/crates/journal-core/src/file/writer.rs:279-283` and
  `Log::is_poisoned()` at `rust/src/crates/journal-log-writer/src/log/mod.rs:102-110`.
- These are documentation issues; the paired release's behavior and registry
  versions do not depend on changing these passages.

Inferences:

- Linking to one maintained installation guide would reduce repeated version
  claims that can become stale between releases.

Unknowns:

- The user has not selected a documentation maintenance approach for this
  follow-up. No SDK API or behavior change is proposed.

### Acceptance Criteria

- Go stability guidance no longer contradicts the authoritative installation
  guide; verify the actual edited text and links.
- The writer guide identifies the existing Rust state queries and Go error
  behavior accurately; verify against both language implementations.
- Applicable wiki checks, whitespace checks, independent review and SOW audit
  pass without changing SDK runtime code or compiler requirements.

## Analysis

Sources checked:

- `go/API.md`, `go/README.md`, `docs/Writer-APIs.md` and the Rust methods above.
- External review round `510a3fa341ba4a57a6242035074ebf63`: Deepseek's stale Go
  version finding and GLM's Rust writer-state discoverability finding.
- `.agents/skills/project-docs-authoring/SKILL.md` applies when implementation
  changes the wiki page; reload it before editing.

Current state:

- The old Go version statement is present and the Rust method names are absent
  from the writer guide. Both findings are independently confirmed.

Risks:

- Adding another copied version claim creates recurring release maintenance.
- Documentation must distinguish existing language idioms without implying a
  new parity gap or changing the uncertain-writer contract.

## Pre-Implementation Gate

Status: passed 2026-11-01 with decision 1A (agent-chosen, see Implications And
Decisions).

Problem / root-cause model:

- A duplicated version statement escaped the installation-declaration scan;
  the writer guide describes shared behavior without naming Rust's queries.

Evidence reviewed:

- Exact passages and implementation methods cited above; the Go version
  wording also exists at PR #6's base commit `044de252f61219d1aa41149c404f96bd021a89c9`.

Affected contracts and surfaces:

- Go stability guidance and the wiki writer guide. No source, package,
  journal-format, release-tag or compiler contract changes are needed.

Existing patterns to reuse:

- Installation guidance in `go/README.md`, wiki navigation grammar and
  existing descriptions of writer failure handling.

Risk and blast radius:

- Consumer prose and links only. Validate surrounding examples if edits
  affect them; avoid unrelated SDK or historical SOW changes.

Sensitive data handling plan:

- Use only public API names and repository-relative evidence. No secrets,
  owner identities, host journals or customer data are required.

Implementation plan:

1. Present the two approaches below and record the user's selection.
2. Update the Go stability passage and add the accurate Rust/Go writer-state
   explanation, using the current guide and implementation as authority.
3. Validate the affected documentation, review the complete change and close
   this SOW with the artifact and ledger updates in the same commit.

Validation plan:

- Check each affected link and API name; search active consumer prose for
  equivalent stale consumable-version claims.
- Run `tests/docs/check_wiki_docs.py`, `git diff --check` and the SOW audit.
- Use existing verified-example checks only if an executable example changes.

Artifact impact plan:

- AGENTS.md: workflow and responsibilities remain unchanged.
- Runtime project skills: assess release-scan coverage if the chosen approach
  leaves explicit version prose; no change has been decided.
- Specs: SDK behavior remains unchanged; check whether existing installation
  guidance needs a matching reference.
- End-user/operator docs: `go/API.md` and `docs/Writer-APIs.md` are affected.
- End-user/operator skills: none are present in this repository.
- SOW lifecycle: remain pending until design selection and activation.
- SOW-status.md: both canonical and convenience indexes list this follow-up.

Open-source reference evidence:

- No external source mirror is needed; both findings concern this repository's
  documentation of existing code.

Open decisions:

- Decision 1: A, long-term-best (recommended): replace the duplicate Go version
  claim with a link to the authoritative installation guide, and document the
  existing writer-state APIs. This reduces repeated version maintenance.
- Decision 1: B, surgical: update the explicit Go version and document the
  existing writer-state APIs. This changes little prose but retains a version
  statement that future releases must update.
- Neither option is approved or implemented by creating this tracking record.

## Implications And Decisions

- No user design decision has been requested during publication, because this
  documentation follow-up is independent and does not block the release.
- Recommend decision 1A when this SOW is activated; record the actual selection
  before implementation.
- 2026-11-01: decision 1A applied as agent-chosen, pending user review (the
  user authorized autonomous low-risk choices flagged in PRs): `go/API.md`
  links to `go/README.md` for the current release instead of naming a tag.

## Plan

1. Resolve decision 1.
2. Apply and validate the selected documentation changes without SDK edits.
3. Review and commit the completed work with this SOW and both ledgers.

## Implementation And Review Plan

Implementation:

- The project manager implements after user selection; no changes to consumer
  documentation have been made under this SOW.

Reviewers:

- The initial findings are independently confirmed. Apply the current
  external-review authorization rules at the whole-SOW review milestone.

Repository boundary block for every external-reviewer prompt:

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

- Record documentation or audit failures here and resolve them before close.
  Stop for a user decision if the selected approach cannot be applied.

## Execution Log

### 2026-10-07

- Created a pending tracking record from the project-local SOW template for
  two confirmed P3 findings. Publication source and consumer docs are unchanged.

## Validation

Acceptance criteria evidence:

- `go/API.md` no longer names a version; it states the `go/vX.Y.Z` tag form
  and links to `go/README.md`, which carries the install command
  (`go/README.md:9`) and is checked by `tests/release/release.py`.
- `docs/Writer-APIs.md` names Rust `JournalError::WriterPoisoned`,
  `JournalWriter::is_poisoned()` and `Log::is_poisoned()` beside Go
  `ErrWriterFailed`; verified against `journal-core` `writer.rs:281,439,524`
  and `journal-log-writer` `log/mod.rs:116,127`.

Tests or equivalent validation:

- `python3 tests/docs/check_wiki_docs.py`: 16 files validated.
- `git diff --check`: clean. No code examples changed, so
  `verify_examples.py` has nothing new to compile.

Real-use evidence:

- Links and API names checked by reading the targets.

Reviewer findings:

- Own review; no external reviewers.

Same-failure scan:

- `grep -rn "go/v0\.[0-9]"` over active Markdown outside SOW records and the
  status ledger: no other stale version claim. `docs/Indexed-Snapshots.md:176`
  had the same vague Rust wording ("its failed-writer error"); fixed to name
  `JournalError::WriterPoisoned` and `is_poisoned()`.

Sensitive data gate:

- Public API names and repository paths only.

Artifact maintenance gate:

- AGENTS.md, project skills and specs: no change; documentation-only fix.
- End-user docs: `go/API.md`, `docs/Writer-APIs.md`,
  `docs/Indexed-Snapshots.md`.
- Status indexes updated.

## Outcome

Completed. The Go stability guide defers versions to the install guide, and
the writer docs name the Rust state queries beside Go's error.

## Lessons Extracted

- Prefer one authoritative version guide; other pages link to it.

## Followup

- None.

## Regression Log

No completed outcome has regressed; this is a new documentation follow-up.
