# SOW-0001 - Project Bootstrap And Orchestration

## Status

Status: completed

Sub-state: bootstrap completed; SOW moved to `done/` with the verified bootstrap commit.

## Requirements

### Purpose

Produce pure SDKs for reading and writing systemd journal files in Rust, Go, Node.js, and Python, with interoperable journal files across all implementations and a shared test, benchmark, and profiling suite.

### User Request

Bootstrap the repository and SOW framework properly, then project-manage the work through external implementer and reviewer agents. The project manager must not personally code or perform technical review for delegated implementation SOWs.

The SDKs must:

- Read existing systemd journal files without linking to external system libraries.
- Write valid systemd journal files without linking to external system libraries.
- Preserve systemd journal concurrency expectations: one writer and multiple readers may operate on the same journal file according to systemd journal rules.
- Share the same behavioral tests across Rust, Go, Node.js, and Python.
- Support cross-language interoperability: files written by any SDK must be readable by every SDK and by compatible systemd tooling when applicable.
- Include journalctl-like commands in all languages.
- Preserve journalctl's existing repeated-match behavior: one key can be specified multiple times, with AND across different keys and OR across values for the same key.
- Include benchmarks, profiling, and optimization work.

### Acceptance Criteria

- Project has root `AGENTS.md`, `CLAUDE.md` and `GEMINI.md` symlinks, `.agents/skills/`, and `.claude/skills`.
- Project has `.agents/sow/{pending,current,done,specs}/`, project-local SOW template, and project-local audit script.
- Active SOW records user decisions and open risks.
- Specs capture current product scope and compatibility boundaries.
- Runtime project skills capture delegated-agent orchestration and journal compatibility workflows.
- Project-local audit runs and remaining issues are resolved or recorded.

### Assistant Understanding

Facts:

- The user wants this repository managed through project-local SOWs and delegated external agents.
- The project manager must not personally implement SDK code or perform delegated technical review.
- Bootstrap work must finish and be committed before implementation SOWs start.

Inferences:

- The first chunk should produce governance, scope, prompts, and phase SOWs only.
- Implementation SOWs may be activated one at a time after bootstrap is complete.

Unknowns:

- No bootstrap-blocking unknowns remain.

## Analysis

Sources checked:

- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, tag `v260.1`, is the baseline compatibility target.
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `man/journalctl.xml`: journalctl documents AND across different fields and OR alternatives for repeated matches on the same field.
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `src/libsystemd/sd-journal/sd-journal.c`: `sd_journal_add_match()` groups same-field matches into OR terms.
- `ktsaou/netdata @ 6a515000ac89`, `src/crates/jf/`: Netdata's Rust `jf` workspace is the reader compatibility layer.
- `ktsaou/netdata @ 6a515000ac89`, `src/crates/journal-core/` and `src/crates/journal-log-writer/`: newer Netdata writer stack.

Current state:

- Repository governance and SOW runtime files are being bootstrapped.
- No SDK implementation has started.

Risks:

- Ambiguous SOW text could cause implementation agents to start the wrong phase or edit outside the repository.
- Missing decision records could cause later reviewers to reopen already-resolved scope questions.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The repository was effectively empty and lacked shared instructions, SOW lifecycle, specs, and project skills. Without these, delegated agents would receive inconsistent context and could drift in scope.

Evidence reviewed:

- Bootstrap repo audit showed no root agent instruction files and no `.agents/` or `.claude/` directories.
- Bootstrap SOW audit showed SOW was not initialized.
- Existing root `SOW.md` recorded initial decisions but was not in project-local SOW layout.
- systemd and Netdata source references listed in `## Evidence`.

Affected contracts and surfaces:

- Repository instructions: `AGENTS.md`, `CLAUDE.md`, `GEMINI.md`.
- SOW framework: `.agents/sow/`.
- Runtime skills: `.agents/skills/project-*/`.
- Product scope spec: `.agents/sow/specs/product-scope.md`.
- Delegated-agent prompts and future implementation SOWs.

Existing patterns to reuse:

- Global bootstrap-repo target state: root `AGENTS.md` plus symlinks for Claude Code and Gemini CLI.
- Global bootstrap-sow project-local SOW layout and audit template.
- Project-specific SOW rule: decisions must be recorded before implementation.

Risk and blast radius:

- High coordination risk if agents receive incomplete or inconsistent scope.
- High filesystem risk if external agents modify source repositories outside this repo.
- Compatibility risk if journalctl daemon-only behavior is confused with file-backed journalctl behavior.
- Security risk if durable artifacts record sensitive data.

Sensitive data handling plan:

- Durable artifacts must not include raw secrets, credentials, bearer tokens, SNMP communities, community/customer data, personal data, non-private customer-identifying IPs, private endpoints, or proprietary incident details.
- Evidence from local source repositories is recorded as upstream repository plus commit and repository-relative paths, not workstation absolute paths.

Implementation plan:

1. Install root agent instructions and cross-tool symlinks.
2. Install project-local SOW directories, template, and audit script.
3. Move the initial SOW into `.agents/sow/current/`.
4. Add product scope spec and runtime project skills.
5. Add repository boundary guardrails, including no changes outside this repo except `/tmp`, with preference for `.local/`.
6. Run audits and resolve bootstrap issues.

Validation plan:

- Run `bash .agents/sow/audit.sh`.
- Run bootstrap-repo audit.
- Run `git status --short`.
- Inspect symlinks and SOW directories.
- Use reviewer agents to review the completed bootstrap and phase plan before implementation work starts.

Artifact impact plan:

- AGENTS.md: created.
- Runtime project skills: created for orchestration and journal compatibility.
- Specs: created product scope spec.
- End-user/operator docs: no end-user SDK docs exist yet; future implementation SOWs must create them.
- End-user/operator skills: none exist yet.
- SOW lifecycle: initialized with this current SOW and future pending phase SOWs.
- SOW-status.md: created as a human-readable project status summary and must be updated when SOW-0001 closes.

Open-source reference evidence:

- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
- `ktsaou/netdata @ 6a515000ac89`

Open decisions:

- No bootstrap-blocking decisions remain.

## Implications And Decisions

1. Canonical Rust writer implementation
   - Decision: use `journal-log-writer` plus `journal-core` as the canonical Rust writer behavior.
   - Implication: `jf` remains the canonical Rust reader compatibility layer, while the SDK public surface must integrate the `jf` reader with the newer writer stack.
   - Risk: reader and writer come from different Netdata Rust crate families, so integration tests must explicitly prove they operate on the same journal files correctly.

2. Upstream systemd test scope
   - Decision: use SDK conformance plus file-backed journalctl behavior.
   - Include: systemd journal file/API tests, importer tests, journal fixtures, corrupted journal fixtures, and journalctl behavior that can run against journal files or journal directories produced by the SDKs.
   - Exclude: journald daemon lifecycle, systemd service management, journal-remote, journal-gatewayd, journal-upload, varlink service APIs, socket activation, FSS daemon setup, and reboot/boot lifecycle tests.
   - Implication: the project validates journal file compatibility and CLI query semantics without expanding into a pure reimplementation of the full systemd journal service ecosystem.
   - Risk: excluded upstream tests may still expose useful edge cases; each excluded test should be inventoried with a reason and any file-level behavior extracted where practical.

3. Compatibility target
   - Decision: pin the baseline compatibility target to systemd `v260.1`.
   - Evidence: upstream systemd tags include `v260.1` as the latest stable tag at decision time, while local `main` is at `261~rc1`.
   - Implication: conformance tests are reproducible and tied to a stable systemd release.
   - Risk: behavior introduced after `v260.1` is not part of the baseline unless added as a separate compatibility track.

4. Writer feature target
   - Decision: final target is full writer support, including compression and Forward Secure Sealing where systemd journal files define it.
   - Implementation approach: the work may be delivered in phases, with earlier phases allowed to support a smaller writer subset while readers and tests define the compatibility envelope.
   - Implication: the final project target is broader than the first implementation milestone.
   - Risk: FSS/sealing and compressed writing add crypto, verification, and compression complexity across four pure-language implementations.

5. Cross-language API shape
   - Decision: provide two API layers: idiomatic SDK APIs for each language, plus a libsystemd-compatible reader facade.
   - Implication: language users get natural APIs while compatibility tests can still exercise libsystemd-like behavior.
   - Risk: the extra facade increases the API surface that must be tested and documented.
   - Exception rule: the facade may be omitted only if a SOW records concrete evidence that it would require native bindings, violate the pure-language policy, or create an unsafe/unrepresentable API in that language.

6. journalctl query syntax and CLI target
   - Decision: systemd journalctl already supports OR semantics for repeated matches on the same key, so no new `KEY in [values]` extension is required.
   - Evidence: `journalctl` documentation states that matches on different fields are ANDed, while matches on the same field are alternatives. `sd_journal_add_match()` also groups same-field matches into an OR term.
   - Decision: implement journalctl rewrites in Rust, Go, Node.js, and Python.
   - Decision: do not implement daemon-only journalctl commands in this project.
   - Implication: journalctl rewrites cover file-backed/query behavior, while daemon-control options return documented unsupported behavior.
   - Risk: users may expect daemon operations from the command name; documentation and CLI errors must be explicit.

7. Third-party dependency policy
   - Decision: pure-language dependencies are allowed; CGO, native Node.js addons, and linking to system journal libraries are not allowed.
   - Implication: compression, hashing, CLI parsing, crypto, and test harness code may use audited pure-language packages.
   - Risk: dependency selection must be reviewed for native build hooks, licensing, maintenance, and security posture.

8. Project management operating model
   - Decision: implementation and technical review will be delegated to external coding/review agents.
   - Preferred implementer: `llm-netdata-cloud/minimax-m2.7-coder`.
   - Fallback implementer hierarchy: `llm-netdata-cloud/qwen3.6-plus`, then `llm-netdata-cloud/glm-5.1`.
   - Reviewer pool: `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`, and `llm-netdata-cloud/glm-5.1`.
   - Gate: a phase does not advance until the implementer has completed the SOW work and reviewers agree the result is production grade or all findings have been resolved through repeated review cycles.
   - Role separation: the project manager maintains SOWs, phase plans, prompts, evidence ledgers, status, and gate decisions, but does not personally code or perform the technical review.

9. Repository boundary for all agents

Decision: no assistant or delegated agent may make changes outside this repository.
Exception: `/tmp` may be used for scratch work, but `.local/` inside this repository is preferred.
Implication: external references may be inspected read-only, but no external repository, home directory, system path, package cache, or service may be edited by any agent.
Risk: prompts must repeat this rule because implementers and reviewers may otherwise modify nearby source checkouts or dependency caches.

10. Chunk commit discipline

Decision: after each implementation chunk/SOW is implemented, reviewed, and verified, prefer committing that chunk before starting the next chunk.
Implication: the project keeps rollback points before later external-agent work can introduce regressions.
Risk: commits must be intentionally staged with explicit paths only; broad staging commands are not allowed.

## Open Decisions

None.

## Plan

1. Normalize root agent instructions and cross-tool symlinks.
2. Install the project-local SOW runtime, template, audit script, specs directory, and project skills.
3. Record product decisions, repository boundary rules, delegated-agent rules, and commit discipline.
4. Create pending phase SOWs for implementation, test inventory, language ports, interoperability, and benchmarks.
5. Run bootstrap audits and external governance reviews.
6. Resolve reviewer governance findings.
7. Mark this bootstrap SOW completed, move it to `done/`, and commit the verified bootstrap chunk with explicit path staging.

## Delegation Plan

Implementer:

- This bootstrap SOW is maintained by the project manager because it defines the project-management runtime itself.

Reviewers:

- Bootstrap governance review is delegated read-only to `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`, and `llm-netdata-cloud/glm-5.1`.

Repository boundary block for every external-agent prompt:

```text
CRITICAL REPOSITORY BOUNDARY:
- DO NOT MAKE CHANGES OUTSIDE THIS REPOSITORY FOR ANY REASON.
- Repository path: current repository root
- You may inspect external references read-only when the task requires it.
- Write, edit, delete, move, reset, checkout, install, generate, cache, or format nothing outside this repository.
- The only write exception outside the repository is /tmp.
- Prefer .local/ inside this repository for scratch work, generated temporary files, cloned references, logs, and working notes.
```

Failure handling:

- Audit failures block SOW closure until repaired and rerun.
- Reviewer findings block SOW closure unless they are fixed or explicitly dispositioned with evidence.
- Reviewer model failures are recorded in this SOW and retried or replaced with another approved reviewer model.

## Execution Log

- 2026-05-23: Recorded decision to use `journal-log-writer` plus `journal-core` as canonical Rust writer behavior.
- 2026-05-23: Recorded decision to use SDK conformance plus file-backed journalctl behavior for upstream systemd test scope.
- 2026-05-23: Recorded decisions for systemd `v260.1` baseline, full final writer target with phased delivery, two-layer API shape, journalctl repeated-key OR semantics, pure-language dependency policy, and delegated project-management operating model.
- 2026-05-23: Initialized local SOW framework, root project instructions, product scope spec, and runtime project skills.
- 2026-05-23: Recorded hard repository boundary: no changes outside this repository by any assistant or delegated agent; `/tmp` is the only exception and `.local/` is preferred.
- 2026-05-23: Created pending phase SOWs for repository/Rust import, systemd test harness, language implementations, interoperability/full writer features, and benchmarks.
- 2026-05-23: Recorded preference to commit each completed and verified chunk before moving to the next chunk.
- 2026-05-23: Ran external bootstrap reviews with `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`, and `llm-netdata-cloud/glm-5.1`.
- 2026-05-23: Dispositioned reviewer findings by adding canonical repository-boundary prompt text, journalctl `+` separator coverage, two-layer API coverage, delegation-plan template support, SOW-status clarity, audit-failure handling, reviewer read-only rules, fallback implementer handling, and language-SOW sequencing guidance.
- 2026-05-23: Replaced shorthand model names with full `opencode` model IDs and defined fallback implementer hierarchy.
- 2026-05-23: Added `SOW-status.md` to the SOW template artifact gates after reviewer feedback.
- 2026-05-23: Added missing template sections to SOW-0002 through SOW-0009 so pending SOWs can be activated without structural cleanup.
- 2026-05-23: Re-ran final external reviews. `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`, and `llm-netdata-cloud/glm-5.1` reported production grade after the final fixes. `llm-netdata-cloud/kimi-k2.6` reported production grade in the previous round, its final finding was fixed, and its last rerun stalled after read-only audit checks; the exact Kimi reviewer PIDs started by this SOW were terminated and the stall was recorded.

## Validation

Acceptance criteria evidence:

- Root `AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, `.agents/skills/`, and `.claude/skills` were created.
- `.agents/sow/{pending,current,done,specs}/`, `.agents/sow/SOW.template.md`, and `.agents/sow/audit.sh` were created.
- Product scope spec was created at `.agents/sow/specs/product-scope.md`.
- Runtime project skills were created under `.agents/skills/project-agent-orchestration/` and `.agents/skills/project-journal-compatibility/`.
- Pending phase SOWs were created for the planned implementation chunks.

Tests or equivalent validation:

- bootstrap-repo audit reported target state met.
- `bash .agents/sow/audit.sh` reported SOW initialization complete and clean.

Real-use evidence:

- Cross-tool instruction bridges resolve to the root project instructions.
- Project-local SOW audit can run from inside the repository.

Reviewer findings:

- `llm-netdata-cloud/kimi-k2.6`: conditionally production grade; required bootstrap commit, `+` separator consistency, usable reviewer evidence, SOW template delegation plan, canonical boundary block, reviewer read-only rule, fallback handling, and audit-failure behavior.
- `llm-netdata-cloud/mimo-v2.5-pro`: conditionally production grade; required bootstrap commit, external review disposition, `+` separator consistency, SOW-status clarity, SOW template delegation plan, and canonical boundary block.
- `llm-netdata-cloud/qwen3.6-plus`: production grade with minor reservations; required external review disposition, SOW-status clarity, `+` separator in language SOWs, and language SOW sequencing.
- `llm-netdata-cloud/glm-5.1`: production grade with conditions; required external review disposition, SOW-status clarity, two-layer API in AGENTS/specs, and language-specific risk notes before activation.
- Final review disposition: all governance findings except the mechanical close/commit sequence were addressed in this SOW. Mimo, Qwen, and GLM reported production grade after the final fixes. Kimi's last complete verdict was production grade with an activation-readiness finding; that finding was fixed by aligning pending SOWs to the template, and the fix was verified by Mimo. Kimi's final rerun produced no new finding before stalling after read-only checks.
- Language-specific risk enrichment remains an activation requirement recorded in SOW-0005 through SOW-0007. SOW-0002 user-decision gate remains intentionally open until that SOW is activated.

Same-failure scan:

- No implementation code exists yet, so code-pattern same-failure scanning is not applicable in this bootstrap SOW.
- Project-local audit scans all current and pending SOW status/directory consistency.

Sensitive data gate:

- Durable artifacts use upstream repository plus commit and repository-relative paths for external source evidence.
- No raw secrets, credentials, bearer tokens, SNMP communities, community member names, customer names, personal data, non-private customer-identifying IPs, private endpoints, or proprietary incident details are intentionally recorded.

Artifact maintenance gate:

- AGENTS.md: created and includes SOW runtime, repository boundary, model orchestration, and chunk commit rules.
- Runtime project skills: created for agent orchestration and journal compatibility.
- Specs: product scope spec created.
- End-user/operator docs: no SDK implementation exists yet; docs are tracked in future implementation SOWs.
- End-user/operator skills: none exist yet.
- SOW lifecycle: bootstrap SOW is completed and moved to `done/`; implementation phase SOWs remain in `pending/`.
- SOW-status.md: updated as a human-readable summary.

Specs update:

- `.agents/sow/specs/product-scope.md` created.

Project skills update:

- `.agents/skills/project-agent-orchestration/SKILL.md` created.
- `.agents/skills/project-journal-compatibility/SKILL.md` created.

End-user/operator docs update:

- No end-user SDK docs exist because no SDK has been implemented yet.

End-user/operator skills update:

- No end-user/operator skills exist yet.

Lessons:

- External-agent prompts must repeat the canonical repository-boundary block verbatim.
- Review output files in `.local/` may be read before long-running reviewer sessions finish, so reviewer evidence should be checked after the processes exit.

Follow-up mapping:

- Implementation work is tracked by pending SOW-0002 through SOW-0009.

## Outcome

Bootstrap completed. The repository now has shared agent instructions, project-local SOW runtime, project scope spec, runtime project skills, pending phase SOWs, clean audits, and external review evidence. The remaining implementation work is tracked in SOW-0002 through SOW-0009.

## Lessons Extracted

- External-agent model IDs must be recorded as the exact `opencode run -m` identifiers, with friendly aliases left only in prose when needed.
- Broad phrases such as "where practical" need explicit exception criteria before implementation agents use them.

## Followup

- Activate SOW-0002 after this bootstrap chunk is completed, moved to `done/`, and committed.

## Regression Log

None yet.
