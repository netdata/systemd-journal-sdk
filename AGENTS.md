# systemd Journal SDK Project Instructions

## Goals

This project produces pure SDKs for reading and writing systemd journal files in Rust, Go, Node.js, and Python.

Success means:

- SDKs read existing journal files without linking to system journal libraries.
- SDKs write valid journal files without CGO, native Node.js runtime addon loading/linking, or external journal libraries. Dependency packages may ship native artifacts if the SDK runtime path is constrained and tested to use only non-native implementations (e.g. WASM).
- Journal files written by one language can be read by every other language and by compatible systemd tooling where applicable.
- The same shared conformance suite, fixtures, interoperability tests, benchmarks, and profiling workflows apply to every implementation.
- journalctl rewrites exist for Rust, Go, Node.js, and Python for file-backed/query behavior.
- Daemon-only journalctl operations are not implemented in this project.

Project SOW status: initialized

## SOW System

This project uses a local Statement of Work system.

The SOW system is self-contained in this repository. Normal SOW work must not depend on `~/.agents`, `~/.AGENTS.md`, global templates, or global scripts. Use this `AGENTS.md`, project-local SOW files, project-local specs, project-local skills, and the active SOW.

### Roles

- **User responsibilities:** purpose, scope decisions, design forks, risk acceptance, destructive approvals, and final product judgment.
- **Project manager responsibilities:** SOW creation, phase planning, prompt writing, external-agent orchestration, reviewer coordination, evidence ledgers, status reporting, and gate enforcement.
- **Implementer agent responsibilities:** code, tests, documentation, benchmark/profiling work, and implementation evidence for the assigned SOW.
- **Reviewer agent responsibilities:** independent technical review, regression search, security review, unwanted side-effect review, and production-grade readiness assessment.

The project manager must not personally perform the terminal technical review for implementation SOWs. By default implementation can be delegated to external agents, but the current user routing decision is local implementation by the project manager with external models used as read-only reviewers only.

Current review cadence: implement the whole active SOW locally, finish local validation, then run external reviewers against the complete SOW as one meaningful batch. Do not run external reviewers after small local edits or partial fixes unless the user explicitly asks for early review, or unless a blocking design/security/compatibility decision needs an independent read-only opinion before implementation can continue.

### Required First Checks

Before non-trivial work:

1. Read pending/current SOWs for overlap, contradictions, and existing decisions.
2. Read relevant specs under `.agents/sow/specs/`.
3. Inspect `.agents/skills/project-*/SKILL.md` and load every runtime project skill whose trigger matches the work.
4. Inspect code/docs/data as ground truth.
5. Ask the user only for irreducible product/design/risk decisions.

### Git Worktrees

Assistants must not create git worktrees on their own. Create a git worktree only when the user explicitly asks for it or approves it.

### Repository Boundary

CRITICAL: Do not make changes outside this repository. This applies to all assistants and all delegated agents.

Canonical external-agent prompt block:

```text
CRITICAL REPOSITORY BOUNDARY:
- DO NOT MAKE CHANGES OUTSIDE THIS REPOSITORY FOR ANY REASON.
- Repository path: current repository root.
- You may inspect external references read-only when the task requires it.
- Write, edit, delete, move, reset, checkout, install, generate, cache, or format nothing outside this repository.
- The only write exception outside the repository is /tmp.
- Prefer .local/ inside this repository for scratch work, generated temporary files, cloned references, logs, and working notes.
```

Rules:

- Read-only inspection outside this repository is allowed when required by a SOW.
- Write, edit, delete, move, reset, checkout, install, generate, cache, or format nothing outside this repository.
- The only write exception is `/tmp`.
- Prefer `.local/` inside this repository for scratch work, generated temporary files, cloned references, logs, and external-agent working notes.
- Every external-agent prompt must include the canonical external-agent prompt block verbatim.

### Sensitive Data In Durable Artifacts

SOWs, specs, documentation, project skills, agent instructions, and code comments are commit-ready artifacts. Treat them as public unless a repository-specific policy explicitly says otherwise.

CRITICAL: Never write raw sensitive data to durable artifacts. This includes passwords, API keys, bearer tokens, SNMP communities, private keys, connection strings with embedded credentials, session cookies, community member names, customer names, customer identifiers, personal data, non-private IP addresses that can identify customers, private endpoints, account IDs, and proprietary incident details.

Write only sanitized evidence:

- use placeholders such as `[REDACTED_SECRET]`, `[CUSTOMER]`, `[ACCOUNT]`, `[PRIVATE_ENDPOINT]`;
- use stable aliases such as `customer-a` only when the real mapping is not stored in the repository;
- cite file paths, line numbers, command names, schema fields, or error classes instead of copying sensitive values;
- summarize logs and traces; include only minimal redacted snippets.

If sensitive data is required to continue, stop and ask the user for a secure handling path. If sensitive data is found in a durable artifact, sanitize it before any commit. If sensitive data was already committed, tell the user and do not rewrite history without explicit approval.

### Open-Source Reference Evidence

When a SOW uses external open-source repositories as evidence, record the upstream repository identity and checked commit, not the workstation mirror path.

For local mirrored or cloned open-source repositories, cite evidence in this form:

```text
owner/repo @ commit
relative/path/inside/repo:line
```

Rules:

- Never use workstation absolute paths for external open-source evidence in SOWs.
- Resolve `owner/repo` from the repository remote, not only from the local directory name.
- Record the commit with `git -C <repo> rev-parse --short=12 HEAD` or the full hash when precision matters.
- Use paths relative to the upstream repository root after the `owner/repo @ commit` line.
- If multiple repositories were checked, list each repository and commit separately.

### Pre-Implementation Gate

Implementation must not begin until the active SOW contains a concrete `## Pre-Implementation Gate` section. Before moving a SOW from `pending/open` to `current/in-progress`, or before continuing implementation in an existing current SOW that lacks this section, fill the gate.

The gate must record:

- Problem / root-cause model.
- Evidence reviewed.
- Affected contracts and surfaces.
- Existing patterns to reuse.
- Risk and blast radius.
- Sensitive data handling plan.
- Implementation plan.
- Validation plan.
- Artifact impact plan.
- Open decisions.

Generic placeholders such as `TBD`, `N/A`, or "to be checked later" are invalid unless the SOW explains why the item truly does not apply. If the gate exposes an unknown that cannot be resolved by investigation, stop and ask the user before implementation.

### When A SOW Is Required

Create or reuse a SOW for non-trivial work:

- feature work;
- bug fixes with behavioral impact;
- refactors;
- migrations;
- documentation or content changes with product/business impact;
- process changes;
- regressions;
- spec hygiene;
- project skill changes;
- any work with unclear risk.

Trivial work does not need a SOW:

- typo fixes;
- formatting-only changes;
- mechanical rename with no behavior change;
- simple search/replace with low risk.

When unsure, treat the work as non-trivial.

### SOW Locations

- Pending: `.agents/sow/pending/`
- Current: `.agents/sow/current/`
- Done: `.agents/sow/done/`
- Specs: `.agents/sow/specs/`
- Template for new SOWs: `.agents/sow/SOW.template.md`
- Local audit: `.agents/sow/audit.sh`

Create new SOW files from `.agents/sow/SOW.template.md`. The template is project-local and may be customized for this repository.

Empty SOW directories must contain `.gitkeep` or `.keep` so the committed repository preserves the full SOW layout after clone/checkout.

Filename:

```text
SOW-NNNN-YYYYMMDD-{slug}.md
```

Status and directory must agree:

- `open` lives in `pending/`
- `in-progress` lives in `current/`
- `paused` lives in `current/`
- `completed` lives in `done/`
- `closed` lives in `done/`

### SOW Completion And Commit

The successful terminal SOW status is `completed`. `done` is a directory name, not a status value. Never write `Status: done` or `Status: complete`.

After each implementation chunk/SOW is implemented, reviewed, and verified, prefer committing that chunk before starting the next chunk. This preserves rollback points before subsequent external-agent work.

If `.agents/sow/audit.sh` fails, do not close the SOW and do not advance to the next chunk. Record the audit failure in the active SOW, repair it inside this repository, rerun the audit, and record the clean result.

When a SOW's work is ready to close:

1. Finish implementation, docs, specs, skills, validation, and follow-up mapping.
2. Update the SOW to `Status: completed`.
3. Move the SOW file to `.agents/sow/done/`.
4. Commit the work, artifact updates, SOW status change, and SOW move together as one commit, unless the user explicitly requested a different commit split.

Do not create a separate commit just to mark or move the SOW. Do not claim a SOW is completed while the implementation and the SOW lifecycle change live in separate uncommitted or separately committed states.

Git staging rule: never use `git add -A` or `git add .`. Stage explicit paths only.

### One SOW At A Time

Never execute multiple SOWs as one batch.

If work overlaps:

- merge or consolidate before implementation; or
- split into separate SOWs and complete one before starting the next.

Progress reports are not stop points. Once a SOW is in progress, continue until it is delivered, failed with evidence, blocked on a real user decision/approval, or superseded by newer user instructions.

### User Decisions

When user decisions are needed:

1. Present concrete evidence with files/lines or source references.
2. Provide numbered options.
3. Explain pros, cons, implications, and risks.
4. Recommend one option with reasoning.
5. Record the user's decision in the SOW before implementation.

### Followup Discipline

"Deferred" is not a terminal outcome.

Before a SOW can close, every valid deferred item must be:

- implemented in the current SOW; or
- explicitly rejected as not worth doing, with evidence; or
- represented by a real pending/current SOW file.

Pre-close, search the SOW for:

```text
defer|later|follow-up|future|TODO|pending
```

Map every remaining item to implemented, rejected, or tracked.

### Regressions

A regression is discovered after a SOW was considered completed or closed, later testing or use finds broken behavior, and the original SOW's claimed outcome is no longer true.

When behavior that a completed SOW claimed working stops working:

1. Find the original SOW in `done/`.
2. Move it back to `current/`.
3. Mark it `in-progress` with a regression note in `## Status`.
4. Append a new dated `## Regression - YYYY-MM-DD` section at the end of the file, after the original outcome, lessons, and follow-up content.
5. In that appended section, record what broke, evidence, why previous validation missed it, the repair plan, validation, and updates needed to specs, skills, docs, audits, or follow-up SOWs.
6. Fix and validate there.

Never prepend regression content above the original SOW narrative. The original requirements, analysis, plan, validation, outcome, lessons, and follow-up must remain readable first.
Do not create a new SOW for a true regression.

### Validation Gate

A SOW cannot be completed until Validation records:

- acceptance criteria evidence;
- tests or equivalent validation;
- real-use evidence when a runnable path exists;
- reviewer findings and how they were handled;
- same-failure search results;
- sensitive data gate;
- artifact maintenance gate;
- SOW status/directory consistency;
- spec update or specific reason no spec update was needed;
- project skill update or specific reason no skill update was needed;
- end-user/operator docs update or evidence-backed reason none were affected;
- end-user/operator skill update or evidence-backed reason none were affected by docs/spec changes;
- lessons extracted or specific reason there were none;
- follow-up mapping.

Generic "N/A" is invalid.

Reviewer agents must be read-only. Reviewer prompts must forbid creating, modifying, deleting, moving, formatting, staging, committing, or changing files.

### Artifact Maintenance Gate

Every SOW close must explicitly record whether each durable artifact class was updated or why no update was needed:

- `AGENTS.md` - workflow, responsibility, local framework, project-wide guardrails.
- Runtime project skills - `.agents/skills/project-*/SKILL.md` for HOW to work here.
- Specs - `.agents/sow/specs/` for WHAT the project does.
- End-user/operator docs - README, docs site, runbooks, published guides, help text, or other human-facing documentation.
- End-user/operator skills - output/reference skills copied or consumed outside normal repo work.
- SOW lifecycle - split, merge, status, directory, deferred work, regression reopening, and follow-up mapping.
- `SOW-status.md` - human-readable project status summary, updated whenever SOW state changes.

### Specs

Specs are memory of WHAT this project does.

Update specs when shipped work changes:

- product behavior;
- public contracts;
- data formats;
- UX rules;
- business logic;
- operational guarantees;
- known edge cases.

Specs describe current reality, not aspiration. If specs and code disagree, record the discrepancy in the active SOW and resolve or track it.

### Project Skills

Project skills are memory of HOW to work here.

Runtime input project skills live under `.agents/skills/project-*/SKILL.md`. The `project-` prefix is the generic hook meaning agents working in this repo must consider this skill. Before non-trivial work, inspect those skill descriptions and load every matching runtime skill.

Runtime input skills:

- `.agents/skills/project-agent-orchestration/SKILL.md`
- `.agents/skills/project-journal-compatibility/SKILL.md`
- `.agents/skills/project-release-tagging/SKILL.md`

Legacy runtime skills:

- None.

Output/reference skills:

- None.

### Project-specific overrides

- The baseline systemd compatibility target is `systemd/systemd` tag `v260.1`.
- The final writer target includes compression and Forward Secure Sealing where systemd journal files define it, but implementation may be phased.
- Readers must handle applicable historical journal file variants covered by the shared conformance fixtures.
- Provide two API layers per language: idiomatic SDK API plus a libsystemd-compatible reader facade. The facade is required unless a SOW records concrete evidence that it would require native bindings, violate the pure-language policy, or create an unsafe/unrepresentable API in that language.
- journalctl repeated matches for the same field already provide OR semantics. No new `KEY in [values]` syntax is required.
- The `+` separator is a systemd journalctl disjunction feature to replicate for file-backed journalctl behavior; it is not a new extension.
- Implement journalctl rewrites for file-backed/query behavior in Rust, Go, Node.js, and Python.
- Do not implement daemon-only journalctl commands, including daemon sync, flush, rotate, and relinquish-var operations.
- Common compression-library dependencies are allowed after dependency review. Journal parsing/writing must not depend on systemd/libjournal; CGO, native Node.js runtime addon loading/linking, and linking to system journal libraries remain disallowed unless the user explicitly changes those separate constraints. Dependency packages may ship native artifacts if the SDK runtime path is constrained and tested to use only non-native implementations (e.g. WASM) and does not load or link native code at runtime.
- Current implementation routing: do implementation locally in this repository; do not run external implementer agents unless the user explicitly changes this decision.
- Reviewer pool: `llm-netdata-cloud/minimax-m2.7-coder`, `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/qwen3.6-plus`, and `llm-netdata-cloud/glm-5.1`. `llm-netdata-cloud/mimo-v2.5-pro` is currently skipped because the user reported it is out of quota.
- Current review cadence: finish the complete active SOW locally first, including local validation and SOW evidence, then run the reviewer pool against the entire SOW and changed surface as one batch. Do not run reviewers after every small edit.
- A phase cannot advance until the local implementation or explicitly approved implementer run has completed the active SOW and reviewer findings have been resolved or explicitly dispositioned in the SOW.
- After each verified chunk, prefer committing the chunk before starting the next work chunk, using explicit path staging only.
- If the user re-enables external implementers, record the routing decision in the active SOW before running them.
- After SOW-0003 completes, SOW-0005 (Go writer first) activates before SOW-0004, SOW-0006, SOW-0007, SOW-0008, SOW-0009, or SOW-0010. After SOW-0005 completes, continue according to the active SOW dependency chain, but only one implementation SOW may be active at a time.
- Reviewer agents may run in parallel within the active SOW; implementation SOWs must not run in parallel unless the user explicitly changes the one-SOW-at-a-time rule.
