# SOW-0076 - Independent Selective Real Corpus Verification

## Status

Status: open

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: tracked from the parallel validation discussion; awaiting activation after the active historical-reader gap is resolved or explicitly waived.

## Requirements

### Purpose

Run an independent, selective verification pass against representative real journal files from the workstation corpus so the SDK is validated against real-world shape, size, compression, active/archived states, and historical variation without requiring a full 100+ GiB regeneration run every time.

### User Request

The user requested a separate agent to selectively verify SDK implementations against a few files from the large local journal corpus, in parallel with other workstreams. The purpose was to gain confidence faster than a full corpus scan while still using real data.

### Assistant Understanding

Facts:

- SOW-0064 implemented the real-corpus harness and performed single-file, focused 100-file, and targeted discrepancy checks.
- The full local corpus is large enough that complete repeated scans can take too long for normal iteration.
- Real journal files may contain sensitive operational data.
- SOW-0073 currently tracks a historical unkeyed/LZ4 reader gap discovered during corpus work.

Inferences:

- A standing selective verification suite should choose files by feature and shape, not by arbitrary filename.
- The selection should include high-value edge classes: large payloads, compressed DATA, compact files, FSS where available, active/open files copied to snapshot, archived files, old unkeyed files, multi-boot files, high-cardinality payloads, and files that previously exposed bugs.
- The selective pass should be independently runnable after important reader/writer changes and before Netdata integration work.

Unknowns:

- Exact selected file set and whether all files remain locally available.
- Whether selected files should be represented by persistent sanitized manifests only, or by generated redacted fixtures.
- Whether systemd regeneration should be included once a safe pipeline is implemented.

### Acceptance Criteria

- Define a sanitized selection policy for representative real corpus files.
- Build or update a manifest of selected files using sanitized stable IDs, feature classifications, sizes, and hashes without raw paths by default.
- Run systemd, Rust, and Go reader digest comparisons against the selected files.
- Run Rust and Go regeneration checks for selected files through supported modes, with stock `journalctl --verify --file` and systemd reread of generated outputs.
- Include active-file snapshot handling so changing source files do not produce false discrepancies.
- Report counts, logical digests, feature classes, elapsed times, memory/I/O metrics where available, and discrepancy codes.
- Never commit raw journal files, raw fields, raw values, hostnames, IPs, usernames, messages, binary payloads, or private paths.
- If a discrepancy is found, create or reopen a focused SOW; this SOW identifies and reports issues rather than absorbing all fixes.
- Provide a short command recipe for rerunning the selective verification after major reader/writer changes.

## Analysis

Sources checked:

- User discussion requesting a selective real-corpus verification agent.
- SOW-0064 corpus evaluation outcome and validation notes.
- SOW-0073 current historical-reader gap.
- Project sensitive-data and repository-boundary rules in `AGENTS.md`.

Current state:

- The harness exists from SOW-0064.
- Some focused real-corpus checks were already run under SOW-0064.
- There is no separate pending SOW for a repeatable independent selective pass.

Risks:

- Raw real journal content can leak if reports are careless.
- File selections can become stale if based on absolute paths or mutable active files.
- A narrow selection can give false confidence if it misses important feature classes.
- Running this before SOW-0073 is closed may rediscover known unkeyed historical gaps.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Full-corpus evaluation is valuable but too expensive to run for every iteration. A curated selective suite can catch real-shape regressions quickly, but it must be tracked as a first-class SOW to avoid being forgotten.

Evidence reviewed:

- SOW-0064 records that focused single-file and 100-file real-corpus checks found and repaired multiple correctness issues.
- SOW-0073 records an active historical unkeyed reader gap that should influence file selection and timing.

Affected contracts and surfaces:

- Real-corpus verification workflow.
- Reader and writer compatibility claims for Rust and Go.
- Regression detection before Netdata integrations.
- Sanitized report artifacts.

Existing patterns to reuse:

- `tests/corpus_eval/run_corpus_eval.py`.
- `tests/corpus_eval/canonical.py`.
- Existing report/sensitive-data patterns from SOW-0064.
- Existing stock `journalctl --file` verification flow.

Risk and blast radius:

- Low implementation blast radius if this stays tooling/reporting only.
- Medium operational load depending on selected file count and regeneration modes.
- High sensitive-data risk if raw content is ever written to durable artifacts; reports must remain hashes/counts only.

Sensitive data handling plan:

- Treat real journals as sensitive.
- Durable artifacts may include only sanitized file IDs, feature classes, sizes, counts, hashes, timings, and status codes.
- Do not commit raw journal paths by default. If path hints are needed, use non-sensitive aliases.
- Keep generated outputs under `.local/` and delete them by default after validation.

Implementation plan:

1. Define feature-based selection policy.
2. Produce a sanitized manifest from local corpus discovery.
3. Run reader parity on selected files.
4. Run supported Rust/Go regeneration modes on selected files.
5. Produce sanitized aggregate and per-file reports.
6. Map discrepancies to follow-up SOWs.

Validation plan:

- Stock systemd, Rust, and Go reader digest parity.
- Stock verification and systemd reread of regenerated outputs.
- Sensitive-data scan of reports.
- `.agents/sow/audit.sh` and `git diff --check`.

Artifact impact plan:

- AGENTS.md: no expected update.
- Runtime project skills: no expected update unless selective corpus verification becomes mandatory before closure of future SOWs.
- Specs: update only if results change compatibility claims.
- End-user/operator docs: likely unaffected.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: pending until activated; discrepancies map to follow-up SOWs.
- SOW-status.md: update now and on completion.

Open-source reference evidence:

- None checked for this tracking SOW. This work validates local real artifacts, not external source behavior.

Open decisions:

- Whether to run before or after SOW-0073 completion.
- Exact selected feature classes and maximum runtime budget.
- Whether to include systemd writer/regeneration once a safe pipeline exists.

## Implications And Decisions

1. 2026-06-01 tracking decision
   - Decision: create this pending SOW so independent selective corpus validation is visible in the work queue.
   - Implication: SOW-0064 remains closed, while this SOW can run later as a repeatable confidence pass.

## Plan

1. Define and document selected corpus classes.
2. Generate sanitized selection manifest.
3. Run reader and writer checks.
4. Publish sanitized report and follow-up mapping.

## Delegation Plan

Implementer:

- To be decided when activated. This SOW is suitable for a dedicated agent because it can run independently once the selection policy is fixed.

Reviewers:

- Reviewer pool after complete implementation and local validation: minimax, kimi, qwen, glm, and mimo.

Repository boundary block for every external-agent prompt:

```text
CRITICAL REPOSITORY BOUNDARY:
- DO NOT MAKE CHANGES OUTSIDE THIS REPOSITORY FOR ANY REASON.
- Repository path: current repository root.
- You may inspect external references read-only when the task requires it.
- Write, edit, delete, move, reset, checkout, install, generate, cache, or format nothing outside this repository.
- The only write exception outside the repository is /tmp.
- Prefer .local/ inside this repository for scratch work, generated temporary files, cloned references, logs, and working notes.
```

Failure handling:

- If selected files disappear or become unreadable, report the selection failure and pick replacements by the documented policy.
- If a known SOW-0073 discrepancy appears before that SOW closes, classify it as known rather than opening a duplicate.

## Execution Log

### 2026-06-01

- Created this pending SOW to track the previously discussed independent selective real-corpus verification stream.

## Validation

Acceptance criteria evidence:

- Pending.

Tests or equivalent validation:

- Pending.

Real-use evidence:

- Pending.

Reviewer findings:

- Pending.

Same-failure scan:

- Pending.

Sensitive data gate:

- This planning artifact contains no raw journal content, hostnames, IPs, usernames, machine IDs, boot IDs, or private paths.

Artifact maintenance gate:

- AGENTS.md: no update needed for tracking.
- Runtime project skills: no update needed for tracking.
- Specs: no update needed until implementation changes compatibility claims.
- End-user/operator docs: no update needed for tracking.
- End-user/operator skills: no update needed for tracking.
- SOW lifecycle: created as `Status: open` under `.agents/sow/pending/`.
- SOW-status.md: updated to list this pending SOW.

Specs update:

- No spec update needed for tracking only.

Project skills update:

- No project skill update needed for tracking only.

End-user/operator docs update:

- No docs update needed for tracking only.

End-user/operator skills update:

- No output/reference skill update needed.

Lessons:

- Real-corpus confidence work needs a repeatable selective suite, not only one-off full or partial sweeps.

Follow-up mapping:

- Tracked by this SOW.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and later testing or use found broken behavior. Use a dated `## Regression - YYYY-MM-DD` heading at the end of the file. Never prepend regression content above the original SOW narrative.
