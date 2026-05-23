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
- `.agents/sow/done/SOW-0003-20260523-systemd-test-inventory-and-shared-harness.md`
- `.agents/sow/done/SOW-0005-20260523-go-sdk-and-journalctl.md`
- `.agents/sow/done/SOW-0011-20260523-live-concurrency-compatibility-gate.md`
- `.agents/sow/done/SOW-0012-20260523-go-writer-binary-fields.md`

## Next SOW

- Recommended next: `.agents/sow/pending/SOW-0010-20260523-go-reader-and-journalctl-completion.md`
- Rationale: after the Go writer live compatibility and binary field gates, the natural next step is completing the Go reader and file-backed journalctl surface while reusing the same live harness.
- Alternative: `.agents/sow/pending/SOW-0004-20260523-rust-sdk-and-journalctl.md` if Rust API finalization should precede the remaining languages.

## Guardrails

- CRITICAL: Never write raw sensitive data to durable artifacts.
- CRITICAL: Do not make changes outside this repository. The only write exception is `/tmp`; prefer `.local/` inside this repository.
- Prefer committing each completed and verified SOW chunk before starting the next chunk. Stage explicit paths only.

## Residuals

- Phase breakdown and implementation SOWs have been created as SOW-0002 through SOW-0009.
- Bootstrap external review ran in repeated rounds; governance findings were dispositioned in SOW-0001.
- SOW-0002 external review completed with all four round-3 reviewers returning `PRODUCTION GRADE`.
- SOW-0002 close commit completed.
- SOW-0003 external review completed with all round-6 reviewers returning `PRODUCTION GRADE`.
- SOW-0003 closeout cleanup completed and is ready for commit.
- 2026-05-23 priority update: after SOW-0003, activate Go writer-first work before Rust, Node.js, Python, full interoperability, or benchmarks.
- SOW-0005 completed the pure-Go writer-first implementation and was committed.
- 2026-05-23 compatibility clarification: live one-writer/multiple-reader compatibility with stock readers is mandatory. SOW-0011 completed the reusable live gate and applied it to the Go writer feature slice.
- 2026-05-23 binary field priority update: SOW-0012 completed binary field compatibility validation for the Go writer before later SDK phases continue.
