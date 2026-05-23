# SOW Deployment Status

This is a human-readable status summary. Canonical state lives in `AGENTS.md`, `.agents/sow/`, and `.agents/sow/audit.sh`.

## Repository

Status: initialized

Bootstrap mode: empty/new project.

## Installed Framework

- Root `AGENTS.md` created.
- `CLAUDE.md` and `GEMINI.md` point to `AGENTS.md`.
- `.agents/skills/` created for project runtime skills.
- `.claude/skills` points to `.agents/skills/`.
- `.agents/sow/{pending,current,done,specs}/` created.
- `.agents/sow/SOW.template.md` installed.
- `.agents/sow/audit.sh` installed.

## Active SOW

- None.

## Completed SOWs

- `.agents/sow/done/SOW-0001-20260523-project-bootstrap-and-orchestration.md`
- `.agents/sow/done/SOW-0002-20260523-repo-scaffold-and-rust-source-import.md`

## Next SOW

- `.agents/sow/pending/SOW-0003-20260523-systemd-test-inventory-and-shared-harness.md`
- Activation is blocked until the shared harness runner format decision is recorded.

## Guardrails

- CRITICAL: Never write raw sensitive data to durable artifacts.
- CRITICAL: Do not make changes outside this repository. The only write exception is `/tmp`; prefer `.local/` inside this repository.
- Prefer committing each completed and verified SOW chunk before starting the next chunk. Stage explicit paths only.

## Residuals

- Phase breakdown and implementation SOWs have been created as SOW-0002 through SOW-0009.
- Bootstrap external review ran in repeated rounds; governance findings were dispositioned in SOW-0001.
- SOW-0002 external review completed with all four round-3 reviewers returning `PRODUCTION GRADE`.
- SOW-0002 close commit completed.
