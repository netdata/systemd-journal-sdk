---
name: project-agent-orchestration
description: "Mandatory workflow when planning, delegating, implementing, reviewing, or closing SOW-driven work through external agents in this repository."
---
# Project Agent Orchestration

## Purpose

Keep implementation and review delegated, reproducible, evidence-based, and bounded to this repository.

## Scope

Use this skill when:

- creating or updating implementation SOWs;
- writing prompts for implementer or reviewer agents;
- deciding whether a phase can advance;
- recording review findings, fixes, validation, or production-grade status.

Do not use this skill for:

- trivial wording-only changes that do not use external agents;
- end-user SDK behavior decisions covered by the journal compatibility skill.

## Mandatory Knowledge

- The project manager does not personally perform the terminal technical review for implementation SOWs. Current user routing (2026-06-11) delegates code implementation to the external implementer model `llm-netdata-cloud/minimax-m3-coder` (fallback `llm-netdata-cloud/glm-5.1`, failure recorded in the active SOW) in normal coding mode; all other pool models are read-only reviewers; only `llm-netdata-cloud` models may be used. The project manager writes documentation prose, validates all delegated work, and owns the outcome. Evidence: `AGENTS.md`.
- Current review cadence is whole-SOW batching: finish the complete active SOW locally, run local validation, update SOW evidence, then run external reviewers against the entire SOW and changed surface as one meaningful batch. Do not run external reviewers after small local edits or partial fixes unless the user explicitly asks for early review, or a blocking design/security/compatibility decision needs an independent read-only opinion before implementation can continue. Evidence: `AGENTS.md`.
- Pool models are `llm-netdata-cloud/glm-5.1`, `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.7-plus`, `llm-netdata-cloud/minimax-m3-coder`, and `llm-netdata-cloud/deepseek-v4-pro`. One pool model implements (currently `minimax-m3-coder`); the implementer never reviews its own SOW; the other five review read-only. Only `llm-netdata-cloud` models may be used. Evidence: `AGENTS.md`.
- CRITICAL: Do not make changes outside this repository. This applies to all assistants and all delegated agents.
- The only write exception outside the repository is `/tmp`; prefer `.local/` inside this repository for scratch files.
- After each implementation chunk is implemented, reviewed, and verified, prefer committing that chunk before starting the next chunk. Stage explicit files only; never use `git add -A` or `git add .`.
- If the user re-enables external implementer agents, they must run in normal coding mode, for example `opencode run -m "<model>" "<prompt>"`. Do not pass `--agent code-reviewer` to implementers because that selects a read-only reviewer role and prevents the requested edits.
- Reviewer agents must run read-only. For opencode reviewer runs, use `--agent code-reviewer` and prompts that forbid creating, modifying, deleting, moving, formatting, staging, committing, or changing files.
- Read-only dependency metadata commands can still write package caches. Prompts that allow dependency inspection must either forbid dependency-fetching commands or require cache/output variables under `.local/` or `/tmp`, including `GOMODCACHE`, `GOCACHE`, `GOPATH`, `npm_config_cache`, `PIP_CACHE_DIR`, `CARGO_HOME`, and equivalent tool caches.
- Journal work must not probe the live host journal. External-agent prompts for journal compatibility work must forbid `systemd-cat`, `logger`, live `journalctl` without `--file` or a repository-local `--directory`, writes to `/var/log/journal` or `/run/log/journal`, and any systemd command that changes host journal state.
- Core SDK runtime work must preserve the four-layer runtime-purity split from `AGENTS.md`: core file-format SDK, systemd/journald compatibility layer, optional identity helper, and optional writer-lock helper. Prompts must not ask agents to put host identity discovery or cooperating-writer locking back into core reader/writer paths.
- Core reader/writer runtime prompts must forbid external programs and host-observation sources in core code, including `/proc`, `/host/proc`, `/etc/machine-id`, platform registries, `sysctl`, `system_profiler`, `ps`, shell commands, subprocess APIs, and equivalent mechanisms. These are allowed only in explicitly named optional helper code and tests for those helpers.

Canonical external-agent prompt block:

```text
CRITICAL REPOSITORY BOUNDARY:
- Do not make changes outside this repository for any reason.
- Repository path: current repository root.
- You may inspect external references read-only when the task requires it.
- Write, edit, delete, move, reset, checkout, install, generate, cache, or format nothing outside this repository.
- The only write exception outside the repository is /tmp.
- Prefer .local/ inside this repository for scratch work, generated temporary files, cloned references, logs, and working notes.
```

## Best Practices

- Split work into small SOWs with one concrete deliverable and clear acceptance gates.
- Work on exactly one active SOW at a time.
- If the user re-enables external implementers, record the routing decision and any model failure in the active SOW before switching models.
- Run independent reviewers in parallel after the whole SOW implementation and local validation are complete.
- Keep reviewer prompts neutral: include the original request, SOW filename, changed scope, validation commands, the canonical repository-boundary block, and ask for unwanted side effects and security issues.
- For SOWs touching runtime purity, ask reviewers to verify that core SDK code has no implicit host identity discovery, subprocess execution, or automatic writer locking, and that optional helpers are opt-in and documented separately.
- For dependency research or package metadata checks, include explicit cache redirection instructions before allowing commands such as `go get`, `go list`, `npm view`, `npm pack`, `pip download`, `pip index`, `cargo metadata`, or `cargo doc`.
- Repeat review cycles with the same whole-SOW scope until reviewers stop finding blocking issues.

## Bad Practices

- Do not let any assistant or external agent edit outside this repository.
- Do not let reviewers make changes; reviewers must be read-only.
- Do not let external agents run package-manager commands with default caches, because they can write under home directories even when the visible command output is read-only.
- Do not narrow follow-up reviewer prompts to only the last fix; keep the original review scope and add fix notes.
- Do not advance a SOW on "mostly ok" or unresolved production-grade doubts.

## Workflow Checklist

1. Confirm the active SOW has a completed pre-implementation gate.
2. Delegate code implementation to the current implementer model recorded in `AGENTS.md`; the project manager writes documentation prose and validates all delegated output.
3. Write the implementer prompt from the SOW, include repository boundary rules, run it in normal coding mode without `--agent code-reviewer`, and capture its summary and changed files.
4. Run reviewer agents in parallel with read-only prompts and reviewer mode only after the whole SOW is locally implemented and validated.
5. Record every finding in the SOW with disposition.
6. Iterate implementation and reviewer cycles until phase gates are satisfied.
7. Run the project-local audit and record results before closing.
8. If the audit fails, repair the issue inside this repository, rerun the audit, and record the clean result before closing.
9. Prefer committing the verified chunk before starting the next work chunk.

## Validation Checklist

Before claiming a phase is ready to advance:

- Active SOW records implementation ownership, reviewer runs, findings, dispositions, and validation.
- Reviewers either state production-grade readiness or all non-production-grade issues are resolved and re-reviewed.
- `.agents/sow/audit.sh` has been run and relevant findings are resolved or recorded.
- No durable artifact contains raw sensitive data.
- No changed file sits outside this repository.
- Verified chunks are committed with explicit path staging before the next chunk starts unless the SOW records why commit was skipped.

## Evidence

- `AGENTS.md`: project roles, model preferences, SOW gates, repository boundary, and external-agent rules.
- `.agents/sow/done/SOW-0001-20260523-project-bootstrap-and-orchestration.md`: initial decisions and operating model.

## Update Rules

Update this skill when:

- the user changes agent model or local/external implementation preferences;
- a review cycle exposes a missed orchestration failure mode;
- repository boundary or scratch-space policy changes.
