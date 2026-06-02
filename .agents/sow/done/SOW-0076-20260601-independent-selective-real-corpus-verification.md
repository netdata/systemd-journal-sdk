# SOW-0076 - Independent Selective Real Corpus Verification

## Status

Status: completed

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: closed after orchestrator review and five read-only reviewer
`PRODUCTION GRADE` votes.

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

2. 2026-06-01 implementation routing decision
   - Decision: activate SOW-0076 for delegated implementation in this
     workspace.
   - Decision: SOW-0076 must not create VMs, provision machines, or modify
     external corpus files. External corpus access is read-only only.
   - Decision: generated raw path manifests, snapshots, regenerated journals,
     and temporary outputs stay under `.local/`; committed reports use
     sanitized IDs and feature classes only.
   - Implication: this SOW can run in parallel with SOW-0075 because it does
     not share VM resources or write outside the repository.

## Plan

1. Define and document selected corpus classes.
2. Generate sanitized selection manifest.
3. Run reader and writer checks.
4. Publish sanitized report and follow-up mapping.

## Delegation Plan

Implementer:

- Implementation worker in this workspace.

Reviewers:

- Reviewer pool after complete implementation and local validation: minimax,
  kimi, qwen, glm, and mimo. This implementation worker did not run external
  reviewers because the assigned prompt asked for implementation and validation
  and final merge/close handoff to the orchestrator.

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
- Moved this SOW to `current/`, set status to `in-progress`, and recorded the
  no-VM/read-only-corpus routing decision.
- Added `tests/corpus_eval/run_selective_real_corpus.py`.
  - Selection uses sanitized IDs and feature classes.
  - Raw local paths are written only to `.local/sow-0076/selective-real-corpus/path-manifest.json`.
  - Active/online files are copied to `.local` snapshots before per-driver
    comparisons and the snapshots are removed afterward.
- Added unit coverage in `tests/corpus_eval/test_selective_real_corpus.py`.
- Added sanitized report destination documentation in
  `tests/corpus_eval/reports/README.md`.
- Ran selection-only discovery against the local real corpus.
  - Discovered 7,195 `.journal` files.
  - Total input bytes: 150,558,354,944.
  - Largest input bytes: 134,217,728.
  - Selected 7 files by feature class.
- Ran full selected verification.
  - Report JSON:
    `tests/corpus_eval/reports/selective-real-corpus-report.json`.
  - Report Markdown:
    `tests/corpus_eval/reports/selective-real-corpus-report.md`.
  - Raw path manifest, not committed:
    `.local/sow-0076/selective-real-corpus/path-manifest.json`.
  - Verification status: `ok`.
  - Results: 77 rows, all `ok`.
  - Reader rows: 21.
  - Writer rows: 56.
  - Discrepancies: 0.
  - Runtime: about 776 seconds.
- Selected feature classes covered:
  - `large-file`
  - `compressed-data`
  - `compact`
  - `active-open-snapshot`
  - `archived`
  - `multi-boot`
  - `high-cardinality`
  - `high-field-count`
- Feature classes not found in the discovered local corpus:
  - `historical-unkeyed`
  - `fss-sealed`
  - `previous-bug-exposure`
- `large-file` is marked covered by the first selected compact/compressed
  archived file, so it is not selected as a separate extra file.
- Python and Node.js were not included. Reason recorded in the report: the
  existing real-corpus harness is Rust/Go/systemd focused and Python/Node
  parity remains mapped to SOW-0065 unless a small language-specific follow-up
  is requested.

## Validation

Acceptance criteria evidence:

- Selection policy:
  - Implemented in `tests/corpus_eval/run_selective_real_corpus.py`.
  - Reported in
    `tests/corpus_eval/reports/selective-real-corpus-report.md`.
- Sanitized manifest/report workflow:
  - Raw path manifest stays under `.local/sow-0076/selective-real-corpus/`.
  - Committed report uses sanitized IDs, feature classes, sizes, hashes,
    counts, timings, memory/I/O metrics, status codes, and discrepancy codes.
- Reader digest comparisons:
  - systemd, Rust, and Go ran on 7 selected files.
  - 21 reader result rows, all `ok`.
  - For every selected file, Rust and Go logical digests matched the systemd
    baseline.
- Regeneration checks:
  - Rust and Go ran `regular`, `compact`, `compact-zstd`, and `compact-fss`
    modes on all selected files.
  - 56 writer result rows, all `ok`.
  - Every generated output passed stock `journalctl --verify --file`; FSS
    outputs used `--verify-key`.
  - Every generated output was reread through systemd and matched the original
    logical digest.
- Active-file snapshot handling:
  - The selected active/online file was classified as
    `active-open-snapshot`.
  - The runner snapshots each selected file under `.local` before driver
    comparisons and removes the snapshot after the case.
- Metrics:
  - Report includes elapsed time, row/payload counts, logical digests,
    process wall/user/system seconds, max RSS, page faults, filesystem I/O
    counters, footprint ratios, and I/O multiplication where available.
- Discrepancy handling:
  - No discrepancies were found; no follow-up SOW was created.
- Rerun recipe:
  - Committed report uses placeholder roots:
    `python tests/corpus_eval/run_selective_real_corpus.py --root <journal-root> [--root <journal-root>] --run-verification`.

Tests or equivalent validation:

- `python -m py_compile tests/corpus_eval/run_selective_real_corpus.py tests/corpus_eval/test_selective_real_corpus.py`
  - Result: passed.
- `python -m unittest tests.corpus_eval.test_selective_real_corpus`
  - Result: passed, 2 tests.
- `python tests/corpus_eval/run_selective_real_corpus.py --root [REDACTED_REAL_CORPUS_ROOT] --root [REDACTED_REAL_CORPUS_ROOT] --object-scan-limit 2000 --boot-probe-limit 64`
  - Result: passed.
  - Selected files: 7.
  - Verification status: `not-run`.
- `python tests/corpus_eval/run_selective_real_corpus.py --root [REDACTED_REAL_CORPUS_ROOT] --root [REDACTED_REAL_CORPUS_ROOT] --object-scan-limit 2000 --boot-probe-limit 64 --run-verification`
  - Result: passed.
  - Selected files: 7.
  - Results: 77.
  - Result statuses: 77 `ok`.
  - Discrepancies: 0.
- `python -m json.tool tests/corpus_eval/reports/selective-real-corpus-report.json`
  - Result: passed.
- Sensitive-marker scan:
  - Command class: `rg` over the committed report and runner for raw journal
    roots, raw field/message markers, host identity markers, and private path
    markers.
  - Result: no matches in the committed report.

Real-use evidence:

- Real corpus discovery and selected verification ran against the local
  workstation corpus read-only.
- The run did not create VMs, did not modify external corpus files, did not
  use live `journalctl` without `--file`, and did not write outside this
  repository except normal process execution.
- Generated outputs, snapshots, state, build caches, and raw path manifest were
  kept under `.local/sow-0076/` and `.local/corpus-eval/` style cache paths;
  raw generated journal files were not staged.

Reviewer findings:

- Read-only reviewer pool ran against the whole SOW and changed surface after
  local validation:
  - `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`.
- Blocking findings: none.
- Non-blocking findings disposition:
  - Missing `historical-unkeyed`, `fss-sealed`, and `previous-bug-exposure`
    source feature classes are explicitly recorded in the report as not found
    in the local corpus; historical and FSS coverage remains provided by the
    systemd matrix and related SOWs.
  - Python and Node.js are intentionally outside this Rust/Go/systemd selective
    pass and remain mapped to SOW-0065.
  - Active-file `byte_sha256` values are discovery-time source hashes. The
    actual comparisons use per-driver active-file snapshots, so this does not
    affect correctness; the committed report also records the
    `active-open-snapshot` feature class.
  - Unit tests are intentionally small because the real acceptance evidence is
    the end-to-end selective corpus run with 77 `ok` result rows.
  - Failure-only exception hashes may fingerprint path-bearing exceptions, but
    this run has no failures and no plaintext error strings in durable reports.

Same-failure scan:

- No discrepancies were found.
- Report sanitizer scan checked for raw path and source-content leak classes
  before SOW update.
- The runner keeps raw paths only in `.local/sow-0076/selective-real-corpus/path-manifest.json`.

Sensitive data gate:

- Passed.
- Committed artifacts contain no raw journal files, raw journal paths, raw
  fields, raw values, hostnames, IPs, usernames, messages, machine IDs, boot
  IDs, private paths, or binary payload dumps.
- Raw path manifest is explicitly marked uncommitted and lives under `.local/`.

Artifact maintenance gate:

- AGENTS.md: no update needed; existing repository-boundary, SOW lifecycle,
  sensitive-data, and no-live-journal rules covered this work.
- Runtime project skills: no update needed; this added a reusable test runner
  but did not change the mandatory workflow for all future SOWs.
- Specs: no update needed; no SDK API, file format, product behavior, or
  compatibility contract changed.
- End-user/operator docs: updated `tests/corpus_eval/reports/README.md` and
  committed the report Markdown with a rerun recipe.
- End-user/operator skills: no update needed for tracking.
- SOW lifecycle: created as `Status: open` under `.agents/sow/pending/`.
- SOW lifecycle: moved to `Status: in-progress` under `.agents/sow/current/`.
- SOW lifecycle: completed and moved to `.agents/sow/done/`.
- SOW-status.md: updated to list this SOW as completed.

Specs update:

- No spec update needed because this SOW adds verification tooling and a
  sanitized report without changing public behavior or guarantees.

Project skills update:

- No project skill update needed. Existing project skills already cover
  repository boundaries, read-only reviewers, sensitive corpus handling, and
  journal compatibility validation.

End-user/operator docs update:

- Added `tests/corpus_eval/reports/README.md`.
- Added `tests/corpus_eval/reports/selective-real-corpus-report.md`.

End-user/operator skills update:

- No output/reference skill exists for this workflow and none was needed.

Lessons:

- Real-corpus confidence work needs a repeatable selective suite, not only one-off full or partial sweeps.
- Active journals must be snapshotted even in a selective pass; otherwise
  source mutation can create false reader or writer discrepancies.
- A feature-based sample is only as good as the local corpus features present.
  This corpus did not provide historical-unkeyed or sealed/FSS source files,
  so those remain covered by systemd matrix evidence rather than this real
  corpus pass.

Follow-up mapping:

- No discrepancy follow-up was created.
- If Python/Node selective real-corpus parity becomes required before SOW-0065,
  create a focused follow-up SOW for a small language-specific reader-only
  pass.

## Outcome

Implementation, real selective verification, and read-only reviewer closure are
complete with 0 discrepancies. SOW is closed as `completed`.

## Lessons Extracted

- Keep the raw path manifest local and make the committed rerun recipe use
  `<journal-root>` placeholders by default.
- Sanitize header evidence down to feature flags and counts; event timing and
  sequence metadata are not needed in durable reports.

## Followup

- None required from this run.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and later testing or use found broken behavior. Use a dated `## Regression - YYYY-MM-DD` heading at the end of the file. Never prepend regression content above the original SOW narrative.
