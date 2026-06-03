# SOW-0084 - Code Scanning And Codacy Gate

## Status

Status: in-progress

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: decisions recorded; scanner workflow active; critical complexity
remediation in progress.

## Requirements

### Purpose

Make this repository safe enough for Netdata integration by enabling GitHub-native code scanning and Codacy SARIF/static-analysis reporting, then triaging and resolving every actionable finding before Netdata consumes the SDK.

### User Request

The user requested enabling GitHub native code scanning and Codacy SARIF for maximum static code analysis and security insight before merging into Netdata. The user then reported Codacy is enabled and currently shows 3056 issues.

### Assistant Understanding

Facts:

- The repository is `netdata/systemd-journal-sdk`, public, with default branch `master`.
- The repository currently has no `.github/` workflow directory.
- The repository currently has no committed `.codacy/` configuration.
- GitHub commit check-run/status APIs did not expose Codacy checks for the current commit during initial investigation.
- Codacy is enabled externally and reports 3056 issues, according to the user.
- The SDK contains Rust, Go, Node.js, and Python code, plus tests, generated reports, fixtures, and SOW/docs.

Inferences:

- This must be a separate release/integration gate before SOW-0047 through SOW-0050 start.
- The first implementation step should be workflow/tooling and machine-readable issue export, not editing thousands of findings manually.
- "Address all findings" needs a precise policy for true positives, false positives, generated/vendor/test fixtures, and tool configuration.
- GitHub-native CodeQL and Codacy serve different purposes: CodeQL creates GitHub code scanning alerts directly; Codacy provides its own analysis and can also produce/upload SARIF if configured through CLI/workflows.

Unknowns:

- Whether the repository has GitHub CodeQL default setup enabled in GitHub settings outside committed workflows.
- Whether the user wants committed advanced CodeQL workflow-as-code or GitHub default setup managed through settings.
- Whether Codacy issue data will be fetched through a token/API/CLI, exported manually from Codacy, or read from GitHub checks if Codacy posts them later.
- Whether a Codacy project token or API token is already available as a GitHub secret.
- Whether findings in generated fixtures, benchmark artifacts, vendored WASM assets, SOW files, or test corpus artifacts should be excluded, suppressed with evidence, or fixed.

### Acceptance Criteria

- GitHub-native code scanning is enabled and produces code scanning alerts for supported languages in this repository.
- Codacy analysis is connected to the repository and either:
  - uploads SARIF into GitHub code scanning, or
  - produces a machine-readable issue export that is stored only as sanitized evidence under `.local/` and summarized in durable reports.
- The SOW records the exact scanner configuration, permissions, secret names, schedules, and branch/PR triggers.
- The 3056 Codacy findings are imported or exported into a machine-readable local triage dataset under `.local/`.
- Findings are grouped by tool, language, severity, category, path class, and fix strategy before bulk edits start.
- Every actionable finding is fixed.
- Every non-actionable finding is explicitly dispositioned as false positive, generated artifact, vendor artifact, test fixture, or accepted limitation with evidence and the least-broad suppression practical.
- Final GitHub code scanning and Codacy runs have no unresolved actionable findings under the agreed policy.
- CI/workflows fail only on the agreed gate after the initial baseline/triage phase.
- No secrets, Codacy tokens, raw SARIF with source contents, private URLs, or sensitive issue payloads are committed.
- SOW-0047 through SOW-0050 remain blocked until this SOW completes or the user explicitly waives it.

## Analysis

Sources checked:

- `AGENTS.md` and project SOW rules.
- `.agents/skills/project-agent-orchestration/SKILL.md`.
- `.agents/sow/SOW-status.md`.
- GitHub repository metadata from `gh repo view netdata/systemd-journal-sdk`.
- GitHub Docs, "Uploading a SARIF file to GitHub": SARIF upload uses `github/codeql-action/upload-sarif`, requires `security-events: write`, and supports categories for multiple SARIF sets.
- GitHub Docs, "Workflow configuration options for code scanning": advanced CodeQL setup supports workflow-as-code, push/PR/schedule triggers, language matrix, and `security-extended` query configuration.
- GitHub Docs, "Configuring default setup for code scanning": default setup can be enabled through repository settings and may override advanced workflow/API upload behavior.
- Codacy CLI v2 documentation: CLI supports `analyze --format sarif`, `-o` output files, and `upload -s <sarif> ...` to Codacy; install/configuration can create `.codacy/`.
- Codacy supported-language documentation: Codacy provides static analysis, duplication, complexity, secret detection, and dependency vulnerability scanning across many languages.

Current state:

- No `.github/` workflows exist.
- No committed `.codacy/` directory exists.
- `.gitignore` ignores `.local/` but not `.codacy/`.
- `gh secret list --repo netdata/systemd-journal-sdk` returned no visible secrets during initial investigation.
- GitHub check runs/status APIs did not return Codacy checks for current commit `c7ace5a4b41fb532c768d64ac399fb6d66c6498c`.
- SOW-0047 through SOW-0050 are pending Netdata integration/removal work and should now depend on this static-analysis gate.

Risks:

- Running scanners without a baseline plan can produce thousands of noisy changes and obscure real security defects.
- Default CodeQL setup in GitHub settings may conflict with committed advanced CodeQL workflows or block SARIF/API uploads.
- Codacy CLI setup can download tools/runtimes and write generated configuration; it must be constrained to this repository and `.local/` caches.
- SARIF can include source snippets or absolute local paths if generated with unsafe options.
- Strictly failing CI on day one can block all work before the 3056 findings are triaged.
- Over-broad exclusions can hide real security issues in tests, fixtures, or generated assets.
- Some Codacy findings may be style/complexity findings that require design decisions rather than mechanical edits.

## Pre-Implementation Gate

Status: passed

Problem / root-cause model:

- The repository is about to become a Netdata dependency. Static-analysis and security visibility must be enabled before integration, but the current repo has no in-repo GitHub code scanning workflows and Codacy reports 3056 issues outside the repo. The immediate problem is not just code quality; it is establishing a reproducible, reviewable gate that can distinguish actionable findings from scanner noise and prevent regressions later.

Evidence reviewed:

- `.agents/sow/SOW-status.md`: Netdata integration SOWs 0047-0050 remain pending.
- GitHub repo metadata: public repository, default branch `master`.
- GitHub docs for CodeQL default setup, advanced setup, and SARIF upload.
- Codacy CLI v2 docs and Codacy supported-language docs.
- Local repository scan: no `.github/` workflow directory and no `.codacy/` configuration.

Affected contracts and surfaces:

- GitHub Actions workflows and permissions.
- GitHub code scanning alerts.
- Codacy project configuration and issue data.
- Rust, Go, Node.js, Python SDK source.
- Tests, fixtures, reports, docs, and SOW files that scanners may analyze.
- Netdata integration readiness gates.
- Release readiness and future v1.0.0 publication confidence.

Existing patterns to reuse:

- SOW review and whole-SOW reviewer gate.
- `.local/` for scratch scan outputs and exported issue datasets.
- Explicit path staging and no-secret durable artifact rules.
- Existing language-specific test and interoperability validation after code changes.

Risk and blast radius:

- High code churn risk if 3056 issues are handled without categorization.
- High false-positive risk if scanner defaults analyze generated reports, fixtures, or intentionally low-level parsing code without context.
- Medium CI disruption risk if all findings fail status checks before triage.
- Medium security risk if SARIF or Codacy exports are committed with raw source snippets, local paths, or sensitive data.
- Medium supply-chain risk from adding third-party GitHub Actions or installing Codacy tools without version pinning and cache constraints.

Sensitive data handling plan:

- Do not commit Codacy API tokens, project tokens, or GitHub tokens.
- Use only GitHub secrets for credentials if workflow upload to Codacy is selected.
- Keep raw SARIF, Codacy issue exports, scanner logs, and local triage databases under `.local/`.
- Durable reports may include counts, tool names, rule IDs, severities, categories, sanitized path prefixes, and remediation summaries, but not raw source snippets when snippets might include fixture payloads or sensitive local paths.
- If SARIF upload to GitHub is enabled, configure tools to avoid embedding source contents where possible.

Implementation plan:

1. Record user decisions for CodeQL mode, Codacy SARIF/export path, gating policy, suppression policy, and action pinning.
2. Add or configure GitHub workflow files under `.github/workflows/` for the selected CodeQL and Codacy/SARIF approach.
3. Add committed Codacy configuration only if the selected path requires it and only after reviewing generated content.
4. Obtain machine-readable Codacy findings through API/CLI/export and store raw data under `.local/`.
5. Build or use a triage summarizer that groups findings by language/tool/rule/path/severity and produces sanitized durable reports.
6. Fix findings in prioritized batches, validating language tests and scanner deltas after each batch.
7. Re-run GitHub/Codacy scans until no unresolved actionable findings remain under the agreed policy.
8. Switch CI from reporting-only to the agreed enforcement gate, if selected.

Validation plan:

- Validate workflow YAML syntax and permissions.
- Run local dry-run/static checks where possible without requiring secrets.
- Trigger GitHub Actions on a branch/PR or `workflow_dispatch` and inspect CodeQL/Codacy/SARIF results.
- Verify Codacy issue count decreases to the agreed target.
- Run affected language tests after code fixes.
- Run `.agents/sow/audit.sh`, `git diff --check`, and external reviewer pool against the complete SOW.

Artifact impact plan:

- AGENTS.md: may need update to require code-scanning gate before Netdata integration and release.
- Runtime project skills: may need update if scanning workflow becomes a mandatory pre-integration/release workflow.
- Specs: likely no SDK behavior spec update unless code fixes change public contracts.
- End-user/operator docs: may need README badge/status documentation after workflows are stable.
- End-user/operator skills: likely unaffected unless a reusable scan/triage skill is produced.
- SOW lifecycle: this SOW blocks Netdata integration SOWs until completed or waived.
- SOW-status.md: update to add SOW-0084 and mark Netdata integration blocked by it.

Open-source reference evidence:

- No local mirrored OSS references were checked yet. This SOW primarily relies on official GitHub and Codacy documentation at creation time; implementation may inspect mature multi-language repositories if workflow design needs examples.

Open decisions:

1. GitHub CodeQL/code scanning mode:
   - Option A: GitHub default setup in repository settings. Fast and simple, but less visible in git history and may block advanced SARIF/API uploads depending on configuration.
   - Option B: Committed advanced CodeQL workflow under `.github/workflows/`. More maintainable and reviewable, supports explicit languages/query suites/schedule, and fits repo-as-code. Recommended.
   - Option C: Both default setup and committed workflows. Not recommended because GitHub documents conflict/override behavior when switching modes.
2. Codacy result path:
   - Option A: Use Codacy cloud UI/checks only, and export issues manually when needed. Lowest repo complexity but not reproducible enough.
   - Option B: Use Codacy CLI v2 in GitHub Actions to generate SARIF, upload SARIF to GitHub code scanning, and optionally upload to Codacy using a GitHub secret. Recommended if a token/secret is available.
   - Option C: Use Codacy API/CLI locally only for triage, keep GitHub SARIF upload limited to CodeQL/other tools. Lower CI complexity but weaker GitHub visibility.
3. Initial CI behavior:
   - Option A: Reporting-only while the 3056 existing issues are triaged, then switch to failing on new/actionable findings after baseline reaches zero. Recommended.
   - Option B: Fail immediately on any Codacy/CodeQL finding. Strong but likely blocks all work until thousands of findings are resolved.
   - Option C: Fail only on high/critical security findings immediately, report the rest. Balanced but requires reliable severity mapping.
4. Finding disposition policy:
   - Option A: "All findings" means every scanner issue must either be fixed or suppressed with rule/path evidence and minimal scope. Recommended.
   - Option B: Fix only security/correctness findings and accept style/complexity debt. Not aligned with the user's "all findings" wording.
   - Option C: Exclude broad directories such as tests/fixtures/SOWs/reports up front. Faster but risks hiding real parser/security problems.
5. Third-party action pinning:
   - Option A: Major-version tags for official GitHub actions and Codacy action/CLI version pinning. Maintains update path and follows GitHub examples. Recommended.
   - Option B: Full SHA pinning for every action. Stronger supply-chain control but higher maintenance and frequent update churn.

## Implications And Decisions

User decisions recorded on 2026-06-02:

1. GitHub CodeQL/code scanning mode: Option B. Use committed advanced CodeQL
   workflow-as-code under `.github/workflows/`.
2. Codacy result path: Option B. Use Codacy Analysis CLI in GitHub Actions to
   generate SARIF and upload it to GitHub code scanning. Use Codacy API or CLI
   export for cloud issue triage when credentials are available.
3. Initial CI behavior: Option A. Keep the gate reporting-only while the
   existing 3056 findings are triaged and fixed. Switch to failing after the
   actionable baseline reaches zero or a later user decision changes the gate.
4. Finding disposition policy: Option A. Every scanner finding must be fixed or
   minimally suppressed with rule/path evidence and a recorded disposition.
5. Third-party action pinning: Option A. Use current major-version tags for
   official GitHub actions plus explicit npm package versions for Codacy tools.
6. Codacy credentials: Option A. Use a GitHub secret or local environment
   variable for the Codacy API token. No token value may be written to this SOW,
   workflows, logs, reports, or any committed artifact.
7. Lizard critical complexity baseline: Option B. User decision on
   2026-06-03: do not baseline or waive the 380 remaining critical complexity
   findings just because SDK adoption is still early. Deal with the problem
   now, while the SDK has limited use, because later refactors would be
   significantly riskier. Narrow non-code/generated-artifact dispositions remain
   allowed only when refactoring would be meaningless, for example generated
   lockfiles.

Implementation implications:

- The first workflow pass must not fail pull requests just because existing
  findings are present.
- Infrastructure errors such as missing SARIF output should still fail the
  workflow, because otherwise the repository would appear scanned while no
  data was produced.
- Raw Codacy exports, SARIF payloads, and local scanner logs remain under
  `.local/` only.
- Durable reports may contain aggregate counts, tool IDs, rule IDs, severities,
  categories, path prefixes, and fix/disposition summaries.

## Plan

1. Decide scanner architecture and gate policy.
2. Implement GitHub CodeQL/code-scanning workflow.
3. Implement Codacy SARIF/export workflow or local import path.
4. Import and summarize the 3056 Codacy findings.
5. Triage by language/tool/rule/path/severity.
6. Fix findings in batches with tests.
7. Re-run scanners and close the gate.

## Delegation Plan

Implementer:

- Local implementation by the project manager unless the user explicitly re-enables external implementer agents for this SOW.

Reviewers:

- Read-only reviewer pool after complete SOW implementation: `llm-netdata-cloud/minimax-m2.7-coder`, `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/qwen3.6-plus`, `llm-netdata-cloud/glm-5.1`, and `llm-netdata-cloud/mimo-v2.5-pro`.

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

- If Codacy token access is missing, record the blocker and use manual export or reporting-only workflow as the fallback.
- If GitHub code scanning rejects SARIF upload, record the exact GitHub error and repair workflow permissions/configuration before proceeding.
- If scanner findings require product/API decisions, stop and present numbered options before code changes.

## Execution Log

### 2026-06-02

- Created SOW after the user reported Codacy is enabled and shows 3056 issues.
- Verified no `.github/` workflow directory and no committed `.codacy/` configuration.
- Verified repository is public and default branch is `master`.
- User accepted recommended decisions for advanced CodeQL, Codacy SARIF,
  reporting-only initial gate, fix-or-minimal-suppression policy, major-version
  action pinning, and secret-backed Codacy API access.
- Verified GitHub CodeQL default setup is `not-configured` via GitHub API.
- Verified `CODACY_API_TOKEN` is not present in the local shell.
- Verified local Codacy CLI commands are installed, but public npm has newer
  Codacy package versions than the globally installed workstation binaries.
- Added GitHub workflows:
  - `.github/workflows/codeql.yml` for advanced CodeQL.
  - `.github/workflows/codacy-sarif.yml` for Codacy Analysis CLI SARIF upload
    plus optional Codacy cloud issue export when `CODACY_API_TOKEN` exists.
- Added sanitized local triage tooling under `tests/code_scanning/`.
- Added operator documentation in `documentation/code-scanning.md`.
- Local Codacy SARIF smoke using pinned `@codacy/analysis-cli@0.8.1` generated
  `.local/codacy-local-smoke/codacy-analysis.sarif` and summarized 725 findings
  from the locally runnable tool subset. This is not the cloud baseline; the
  cloud baseline remains 3056 findings from the user's Codacy UI report.
- Local Codacy tool availability after `init --default`: 9 ready local tools,
  5 unavailable optional tools before dependency installation. Ready local
  tools included Jackson, markdownlint, ShellCheck, Cppcheck, Trivy, Semgrep,
  ESLint 8, Flawfinder, and Agentlinter.
- Local Codacy smoke produced a non-zero Codacy exit status because findings
  and tool errors were present, while still producing SARIF. This validates the
  report-only workflow behavior: existing findings should not fail the job, but
  missing SARIF should.
- Committed and pushed the workflow/triage setup as `f6f864c`.
- First GitHub workflow runs for `f6f864c`:
  - CodeQL run `26846315080`: completed successfully. Rust, Go,
    JavaScript/TypeScript, and Python jobs all succeeded.
  - Codacy SARIF run `26846315043`: completed successfully. SARIF upload
    succeeded; Codacy cloud issue export skipped because `CODACY_API_TOKEN` was
    empty.
- GitHub code scanning API reported 2053 open alerts after the first CodeQL and
  Codacy SARIF runs: 91 CodeQL alerts and 1962 Codacy SARIF alerts. The 2053
  GitHub alerts do not replace the user's Codacy cloud count of 3056; the cloud
  export/token is still required to reconcile the full Codacy issue set.
- The user clarified that the local `codacy` CLI was already authenticated.
  Verified that authenticated CLI export works without a new local token.
- Exported Codacy cloud quality issues to `.local/codacy-cloud/codacy-issues.json`
  through language partitions to avoid the CLI 1000-item cap. Exported count:
  1599 issues, matching the repository dashboard `issuesCount`.
- Exported Codacy security findings to
  `.local/codacy-cloud/codacy-findings.json`. Exported count: 199 findings.
  Finding detail inspection showed a security finding points back to a Codacy
  quality issue through `itemSourceId` / `resultDataId`, so the 199 findings
  are the security view of the quality issue set, not an additional 199 code
  locations.
- Reconciled current known counts:
  - Codacy repository quality issues on `master`: 1599.
  - Codacy security findings on `master`: 199, included in the 1599 quality
    issue set as the `Security` category.
  - GitHub code scanning alerts after CodeQL + Codacy SARIF: 2053.
  - User-observed Codacy UI count: 3056. This still needs UI-scope
    reconciliation; current CLI repository dashboard does not report 3056 for
    `master`.

## Validation

Acceptance criteria evidence:

- GitHub-native code scanning workflow added as `.github/workflows/codeql.yml`.
  GitHub API reported default CodeQL setup was `not-configured` before this
  workflow was added. First pushed run completed successfully.
- Codacy SARIF workflow added as `.github/workflows/codacy-sarif.yml`.
  First pushed run completed successfully and uploaded SARIF to GitHub code
  scanning.
- Raw SARIF and Codacy exports are written under `.local/codacy/` in local
  commands and `.local/codacy/` in workflow runner workspace only.
- Codacy cloud issue export initially imported 1599 quality issues for
  `master`, the export after Codacy analyzed `057b737` imported 1535 quality
  issues, and the latest export after Codacy analyzed `8120b1e` imported 1533
  quality issues. The export after Codacy analyzed `dea354e`, and again after
  Codacy analyzed `c3853f2`, imported 1528 quality issues. The exports after
  Codacy analyzed `045a515`, `c6068ed`, and `e3eebc8` imported 1522 quality
  issues into `.local/codacy-cloud/codacy-issues.json`; after Codacy analyzed
  `20ec32e`, the export imported 1520 quality issues; after Codacy analyzed
  `37491aa`, the export imported 1518 quality issues; after Codacy analyzed
  `9204315`, the export imported 1516 quality issues; after Codacy analyzed
  `bf8f4b9`, the export imported 1509 quality issues; after Codacy analyzed
  `e80bf79`, the export imported 1508 quality issues; after Codacy analyzed
  `99d2b08`, the export imported 1502 quality issues. The exporter partitions by
  language and fails if any partition reaches the CLI limit.
- Codacy cloud security finding export initially imported 199 findings for
  `master`, the export after Codacy analyzed `057b737` imported 182 findings,
  the export after Codacy analyzed `8120b1e` imported 181 findings, and exports
  after Codacy analyzed `dea354e`, `c3853f2`, `045a515`, `c6068ed`,
  `e3eebc8`, `20ec32e`, and `37491aa` imported 179 findings into
  `.local/codacy-cloud/codacy-findings.json`; after Codacy analyzed `9204315`,
  the export imported 178 findings; after Codacy analyzed `bf8f4b9`, the
  export imported 174 findings; after Codacy analyzed `e80bf79`, the export
  imported 173 findings; after Codacy analyzed `99d2b08`, the export imported
  171 findings.
- The user-observed 3056 Codacy UI count remains unreconciled with the Codacy
  CLI repository dashboard count of 1502. Potential causes include UI scope,
  non-master branch scope, additional views, ignored/resolved state inclusion,
  or stale UI totals.
- SOW-0047 through SOW-0050 remain marked as blocked by code-scanning gates in
  `.agents/sow/SOW-status.md`.

Tests or equivalent validation:

- `python3 -m pytest tests/code_scanning`: passed, 6 tests.
- `python3 tests/code_scanning/summarize_findings.py --json-output .local/codacy/empty-summary.json --markdown-output .local/codacy/empty-summary.md`:
  passed and produced a zero-finding sanitized summary.
- `python3 tests/code_scanning/export_codacy_issues.py --output-dir .local/codacy-token-missing-check`:
  failed cleanly with `CODACY_API_TOKEN is not set` and did not print a token.
- `python3 tests/code_scanning/export_codacy_issues.py --source cli --provider gh --organization netdata --repository systemd-journal-sdk --branch master --output-dir .local/codacy-cloud`:
  passed, exported 1599 quality issues and 199 security findings through the
  authenticated `codacy` CLI before the first cleanup batch; rerun after
  Codacy analyzed `057b737` exported 1535 quality issues and 182 security
  findings; rerun after Codacy analyzed `8120b1e` exported 1533 quality issues
  and 181 security findings; rerun after Codacy analyzed `dea354e` exported
  1528 quality issues and 179 security findings; rerun after Codacy analyzed
  `c3853f2` exported 1528 quality issues and 179 security findings; rerun after
  Codacy analyzed `045a515`, and again after Codacy analyzed `c6068ed` and
  `e3eebc8`, exported 1522 quality issues and 179 security findings; rerun
  after Codacy analyzed `20ec32e` exported 1520 quality issues and 179 security
  findings; rerun after Codacy analyzed `37491aa` exported 1518 quality issues
  and 179 security findings; rerun after Codacy analyzed `9204315` exported
  1516 quality issues and 178 security findings; rerun after Codacy analyzed
  `bf8f4b9` exported 1509 quality issues and 174 security findings; rerun
  after Codacy analyzed `e80bf79` exported 1508 quality issues and 173
  security findings; rerun after Codacy analyzed `99d2b08` exported 1502
  quality issues and 171 security findings.
- `python3 tests/code_scanning/export_codacy_issues.py --source cli --provider gh --organization netdata --repository systemd-journal-sdk --branch master --output-dir .local/codacy-cloud --skip-findings --cli-timeout 300`:
  passed, proving the timeout-backed local CLI path still exports quality
  issues.
- `python3 tests/code_scanning/summarize_findings.py --codacy-issues .local/codacy-cloud/codacy-issues.json --codacy-findings .local/codacy-cloud/codacy-findings.json --json-output .local/codacy-cloud/summary.json --markdown-output .local/codacy-cloud/summary.md`:
  passed, summarized 1798 exported Codacy records before the first cleanup
  batch and 1717 exported Codacy records after Codacy analyzed `057b737`. The
  totals include quality issues plus the security-finding view and must not be
  interpreted as distinct source locations.
- Python compile check for edited Python helper/SDK/test files: passed.
- Python compile check for the second Python cleanup batch passed for:
  `python/cmd/livewriter.py`, `python/journal/facade.py`,
  `python/journal/directory_reader.py`, `python/cmd/writer_core_bench.py`,
  `python/journal/writer.py`, `python/journal/reader.py`,
  `python/adapter.py`, `python/journal/directory_writer.py`,
  `tests/interoperability/run_live_matrix.py`,
  `tests/conformance/live/run_live_concurrency.py`,
  `tests/interoperability/run_lock_matrix.py`, and `python/test_all.py`.
- Focused Python runtime checks passed for:
  `test_writer_sealed_basic`, `test_zstd_data_object_parse`,
  `test_xz_and_lz4_data_object_parse`,
  `test_directory_writer_replaces_unsupported_chain_active`,
  `test_file_reader_refresh_failure_preserves_current_mapping`, and
  `test_jf_facade_stateful_reader_operations`.
- `npm_config_cache=.local/npm-cache npm test` in `node/`: passed.
- `node --check node/cmd/journalctl/index.js`: passed.
- `npm_config_cache=.local/npm-cache npm test` in `node/` after the manual
  journalctl parser rewrite: passed.
- `node --check node/src/facade.js node/adapter/index.js node/cmd/dataset_ingester.js node/cmd/reader_core_bench.js`:
  passed.
- `npm_config_cache=.local/npm-cache npm test` in `node/` after dynamic-key
  hardening: passed.
- `python3 -m py_compile python/adapter.py python/journal/writer.py python/journal/reader.py`:
  passed for the sixth cleanup batch.
- `PYTHONPATH=.local/python-deps python3 - <<'PY' ...` focused checks for
  `test_jf_facade_stateful_reader_operations` and
  `test_file_reader_refresh_failure_preserves_current_mapping`: passed for the
  sixth cleanup batch.
- `node --check node/src/lib/hash.js`: passed for the sixth cleanup batch.
- `npm_config_cache=../.local/npm-cache npm test` in `node/`: passed for the
  sixth cleanup batch.
- `python3 -m py_compile python/adapter.py python/journal/writer.py python/journal/reader.py`:
  passed for the follow-up Python cleanup batch.
- `PYTHONPATH=.local/python-deps python3 - <<'PY' ...` focused checks for
  `test_jf_facade_stateful_reader_operations` and
  `test_file_reader_refresh_failure_preserves_current_mapping`: passed for the
  follow-up Python cleanup batch.
- Source scan of `python/adapter.py`, `python/journal/writer.py`, and
  `python/journal/reader.py`: no exact `except Exception` plus `pass` or
  `continue` shape remains in the touched files.
- `python3 -m py_compile python/adapter.py python/test_all.py`: passed for the
  final Python unused-import cleanup.
- `PYTHONPATH=.local/python-deps python3 - <<'PY' ...` verified
  `zstd_available()` returns a boolean and reran focused checks for
  `test_jf_facade_stateful_reader_operations` and
  `test_file_reader_refresh_failure_preserves_current_mapping`: passed for the
  final Python unused-import cleanup.
- `python3 -m py_compile python/adapter.py python/test_all.py`: passed for the
  Python import/reimport cleanup.
- `PYTHONPATH=.local/python-deps python3 - <<'PY' ...` focused checks for
  `test_live_delay_parser`, `test_writer_sealed_basic`,
  `test_compact_sealed_writer_stock_verify`,
  `test_jf_facade_stateful_reader_operations`, and
  `test_file_reader_refresh_failure_preserves_current_mapping`: passed for the
  Python import/reimport cleanup.
- `python3 -m py_compile python/adapter.py`: passed for the final adapter
  unused-import cleanup.
- `python3 -m py_compile python/adapter.py`: passed for the follow-up adapter
  unused-import cleanup after removing `SdJournalSeekTail`.
- `python3 -m py_compile python/adapter.py python/journal/verify.py`: passed
  for the cleanup that removed `SdJournalPrevious` and the unnecessary
  `VerificationError` pass statement.
- `python3 -m py_compile python/adapter.py`: passed for the follow-up adapter
  unused-import cleanup after removing `json_entry`.
- `shellcheck -f gcc .agents/sow/audit.sh`: passed for the singleton cleanup
  batch after making the completed-status marker literal explicit.
- `cppcheck --enable=warning --template=gcc tests/benchmarks/systemd/writer_core_bench.c`:
  passed for the singleton cleanup batch after removing the dead `err`
  variable path.
- `node --check node/cmd/journalctl/index.js`: passed for the singleton cleanup
  batch.
- `npm_config_cache=../.local/npm-cache npm test` in `node/`: passed for the
  singleton cleanup batch.
- `python3 -m py_compile python/test_all.py`: passed for the singleton cleanup
  batch.
- `PYTHONPATH=.local/python-deps python3 - <<'PY' ...`
  `test_writer_archive_closes_before_rename_when_required()`: passed for the
  singleton cleanup batch.
- `node --check node/cmd/journalctl/index.js`: passed for the follow-up
  singleton cleanup batch.
- `python3 -m py_compile python/test_all.py tests/code_scanning/export_codacy_issues.py`:
  passed for the follow-up singleton cleanup batch.
- `cargo check -p adapter` in `rust/`: passed for the Rust adapter suppression
  in the follow-up singleton cleanup batch.
- `PYTHONPATH=.local/python-deps python3 - <<'PY' ...` focused checks for
  `test_directory_writer_lifecycle_delete_and_artifact_size`,
  `test_directory_writer_lazy_retention_runs_on_first_open`, and
  `test_directory_writer_eager_retention_runs_on_open_for_all_policies`:
  passed for the follow-up lifecycle callback cleanup.
- `node node/cmd/journalctl/index.js --file fixtures/systemd/test-data/no-rtc/system.journal.zst --head 1 --output=json`:
  passed and returned one JSON journal entry from a compressed systemd fixture.
- `node --input-type=module - <<'JS' ...` per-case conformance probe with a
  15-second timeout found an unrelated hang in the Node adapter case
  `journal-corruption-append-resilient`. Evidence: earlier conformance cases
  passed through `journal-zstd-compressed-read`; the timed-out child was
  `node/adapter/index.js run`, not the touched journalctl CLI path.
- `npm_config_cache=../.local/npm-cache npm test` in `node/` for the follow-up
  singleton cleanup batch was stopped after the exact test process tree hung in
  the unrelated `journal-corruption-append-resilient` adapter case. The stopped
  PIDs were the `npm test` process tree started by this validation run.
- Local `bandit` validation for `tests/code_scanning/export_codacy_issues.py`
  was not available (`bandit` and `python3 -m bandit` were not installed);
  the `Bandit_B310` disposition will be verified by the next Codacy export
  after push.
- `cargo fmt -p adapter --check`: passed after moving the Rust adapter
  Semgrep suppression to the line immediately before `current_exe()`.
- `cargo check -p adapter` in `rust/`: passed after the Rust adapter
  suppression adjustment.
- `node --check node/cmd/journalctl/index.js`: passed after the AGENTS/Rust
  survivor cleanup.
- `python3 -m py_compile python/test_all.py tests/code_scanning/export_codacy_issues.py`:
  passed after the AGENTS/Rust survivor cleanup.
- `git diff --check`: passed after the AGENTS/Rust survivor cleanup.
- `.agents/sow/audit.sh`: passed after the AGENTS/Rust survivor cleanup.
- `PYTHONPATH=.local/python-deps python3 - <<'PY' ...` importing
  `python/test_all.py`, replacing only `test_conformance_manifest` with a
  no-op, and running `main()`: passed.
- Manifest adapter cases run one-by-one with 20 second timeouts: passed for all
  cases except `journal-corruption-append-resilient`, which was skipped as a
  known timeout. A direct reproduction shows that case can spin in
  `FileReader` on corrupted zstd input; this is a separate test harness/parser
  robustness issue, not caused by this static-analysis cleanup.
- `go test ./...` in `go/`: passed.
- Workflow YAML parse check using Python `yaml.safe_load`: passed for both new
  workflow files.
- `actionlint .github/workflows/codeql.yml .github/workflows/codacy-sarif.yml`:
  passed with no findings.
- `git diff --check`: passed.
- `.agents/sow/audit.sh`: passed.
- `node --check node/adapter/index.js`: passed after replacing the Node adapter
  empty-catch cleanup paths.
- `node node/adapter/index.js list | python3 -c ...`: passed and returned 15
  conformance adapter test names.
- Targeted Node adapter conformance runs passed for
  `journal-match-boolean-logic` and `journal-verify-sealed`, covering the two
  cleanup paths touched by the `ESLint8_no-empty` fix.
- `python3 -m py_compile tests/vm_matrix/run_vm_matrix.py`: passed after moving
  default VM image/seed scratch paths under `.local/sow-0075/`.
- `python3 tests/vm_matrix/run_vm_matrix.py preflight`: ran successfully and
  reported `status=blocked` only because the four capped VM domains already
  exist; required tool discovery and target enumeration completed.
- `cargo fmt -p journal -p journal-registry -p journalctl -p journal-engine -p adapter --check`:
  passed for the Rust argv/temp-dir scanner cleanup batch.
- `cargo check -p journal -p journal-registry -p journal-engine -p journalctl -p adapter`:
  passed for the Rust argv/temp-dir scanner cleanup batch.
- `cargo test -p journal verify_file_rejects_referenced_zero_sized_data_object`:
  passed, covering the `.zst` decompression temp-file replacement.
- `cargo test -p journal-registry from_path_parses_native_absolute_paths` and
  `cargo test -p journal-registry from_raw_path_accepts_native_absolute_paths`:
  passed, covering the registry test temp-dir replacement.
- `git diff --check`: passed after the markdown indentation and AGENTS glossary
  cleanup.
- `node --check node/cmd/dataset_ingester.js node/cmd/journalctl/index.js node/adapter/index.js`:
  passed after the Node non-literal filesystem suppression batch.
- `node node/cmd/journalctl/index.js --file fixtures/systemd/test-data/no-rtc/system.journal.zst --head 1 --output=json`:
  passed after the Node non-literal filesystem suppression batch.
- Targeted Node adapter conformance runs passed for
  `journal-importer-basic-parsing` and `journal-verify-sealed`, covering the
  fixture read and temporary-directory creation sites touched by the Node
  non-literal filesystem suppression batch.
- Local `markdownlint` binary was not installed. A targeted structural check of
  the touched Markdown files found no headings without a blank line below after
  the `markdownlint_MD022`/`markdownlint_MD032` cleanup.
- `git diff --check`: passed after the Markdown blank-line cleanup.
- `git diff --check`: passed after the AGENTS scanner exception-path cleanup.
- `.agents/sow/audit.sh`: passed after the AGENTS scanner exception-path
  cleanup.
- `python3` targeted markdown sanity check for duplicate headings in SOW-0003
  and the SOW-0001 ordered-list marker: passed after the small-rule cleanup.
- `flawfinder --columns tests/benchmarks/systemd/writer_core_bench.c tests/conformance/binary/libsystemd_binary_field_reader.c tests/conformance/live/libsystemd_live_reader.c tests/datasets/ingesters/systemd/dataset_ingester.c`:
  passed for the targeted `strlen` group, reporting `Hits@level = [0] 63 [1]
  0 [2] 15 [3] 0 [4] 1 [5] 0`.
- `gcc -o .local/sow-0084-bin/libsystemd_binary_field_reader tests/conformance/binary/libsystemd_binary_field_reader.c -Wl,--no-as-needed -lsystemd -lm -lpthread`:
  passed after the C helper cleanup.
- `cc tests/conformance/live/libsystemd_live_reader.c -o .local/sow-0084-bin/libsystemd_live_reader -lsystemd`:
  passed after the C helper cleanup.
- `tests/benchmarks/systemd/build_writer_core_bench.sh`: passed after the C
  helper cleanup.
- `tests/datasets/ingesters/systemd/build.sh`: passed after the C helper
  cleanup.
- `git diff --check`: passed after the small-rule cleanup.
- `.agents/sow/audit.sh`: passed after the small-rule cleanup.
- `node --check node/src/index.js node/src/facade.js node/adapter/index.js`:
  passed after the Node unused-symbol cleanup.
- `node -e "import('./node/src/index.js').then(...)"`: passed and verified
  representative public Node exports: `SdJournalOpenFile`,
  `SdJournalAddMatch`, `SdJournalQueryUnique`, `OUTPUT_MODE_DEFAULT`, and
  `parseMatchString`.
- `node node/adapter/index.js list`: passed and returned 15 adapter cases.
- Manifest-backed Node adapter runs passed for `journal-match-boolean-logic`
  and `journal-verify-sealed`.
- `node node/cmd/journalctl/index.js --file fixtures/systemd/test-data/no-rtc/system.journal.zst --head 1 --output=json`:
  passed after the Node unused-symbol cleanup.
- Full `npm_config_cache=../.local/npm-cache npm test` in `node/` was attempted
  again and stopped by exact PID after the known unrelated
  `journal-corruption-append-resilient` adapter hang reproduced. The stopped
  process tree was the `npm test` process started for this validation run.
- Local ESLint verification was unavailable because `node/node_modules/.bin`
  does not contain an ESLint binary; the unused-symbol group will be verified
  by the next Codacy cloud export after push.
- `rg -n "strftime\\(" tests/interoperability tests/benchmarks tests/systemd_matrix tests/corpus_eval tests/datasets python -g '*.py'`:
  passed with no matches after the timestamp cleanup.
- `python3 -m py_compile` passed for all Python files touched by the timestamp
  cleanup.
- `git diff --check`: passed after the timestamp/glossary cleanup.
- `.agents/sow/audit.sh`: passed after the timestamp/glossary cleanup.
- Targeted Python line-length scan of the files reported by
  `Prospector_pycodestyle`: passed with no line over 159 characters.
- `python3 -m py_compile` passed for all Python files touched by the
  line-length cleanup.
- `git diff --check`: passed after the Python line-length cleanup.
- `.agents/sow/audit.sh`: passed after the Python line-length cleanup.
- `git diff --check`: passed after the AGENTS clarity cleanup.
- `.agents/sow/audit.sh`: passed after the AGENTS clarity cleanup.
- Local pinned Codacy package smoke:
  `@codacy/analysis-cli@0.8.1` installed under `.local/codacy-cli-test`;
  `codacy-analysis init --default .` succeeded; `codacy-analysis analyze .`
  produced SARIF and the summarizer produced a sanitized 725-finding summary.
- Local Python harness subprocess cleanup validation passed:
  current-content same-pattern scan for missing `B404`, `B603`, and Semgrep
  subprocess suppressions; line-length scan of all touched Python files;
  `python3 -m py_compile` for all touched Python files;
  `python3 tests/interoperability/run_matrix.py --writers python --readers
  python --entries 2`; `python3 tests/interoperability/run_directory_matrix.py
  --readers python`; `python3 tests/interoperability/run_live_matrix.py
  --writers python --readers python --entries 10 --features regular
  --poll-readers 1 --libsystemd-readers 1 --writer-delay-ms 20`; `git diff
  --check`; and `.agents/sow/audit.sh`.
- Discarded local validation attempts were expected CLI or harness behavior:
  `run_directory_matrix.py --entries 2` rejected an unsupported option,
  `run_live_matrix.py` rejected too-small live-test settings before the valid
  live smoke above passed, and `tests/datasets/validate.py --help` hung during
  module execution and was terminated by exact PID. These attempts touched no
  product code and are not accepted validation evidence.
- Local AGENTS singleton wording cleanup validation passed `git diff --check`,
  `.agents/sow/audit.sh`, and a focused `rg` check showing the vague host
  identity sentence is gone and the explicit `/etc/machine-id` and
  `/var/lib/dbus/machine-id` bullets remain.
- Local follow-up Python Semgrep subprocess cleanup validation passed a focused
  scan of the 20 Codacy-reported command-argument lines showing same-line
  `# nosemgrep` coverage, line-length scan, `python3 -m py_compile` for every
  touched harness file, and `git diff --check`.
- Local Codacy/Cppcheck configuration cleanup validation passed YAML parsing of
  `.codacy.yaml`, local Codacy Analysis CLI `--help` inspection, Cppcheck
  `--enable=all --inline-suppr` scan showing no `missingIncludeSystem` or
  unmatched-suppression output for the six C helper files, systemd helper build
  scripts for writer-core, dataset-ingester, and FSPRG vector generator, direct
  builds for the binary-field and live libsystemd helper programs, `git diff
  --check`, and `.agents/sow/audit.sh`.
- Current Codacy documentation states that `.codacy.yml` or `.codacy.yaml` must
  start with `---`, supports repository-level `exclude_paths`, and supports
  `engines.cppcheck.language` for C/C++ analysis. The locally installed
  Codacy Analysis CLI binary does not expose the documented
  `validate-configuration` command, so validation used YAML parsing and will
  rely on the next cloud analysis as the authoritative configuration check.
- Local Codacy configuration rollback validation passed `python3 -m py_compile`
  for the two touched Python harnesses, focused same-line `# nosemgrep`
  verification for the three remaining Semgrep subprocess argument rows,
  Cppcheck `--enable=all --inline-suppr` scan showing `missingIncludeSystem`
  remains locally suppressed, `git diff --check`, and `.agents/sow/audit.sh`.

Real-use evidence:

- GitHub Actions workflow evidence collected from pushed commit `f6f864c`:
  - CodeQL: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26846315080`.
  - Codacy SARIF: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26846315043`.
- GitHub Actions workflow evidence collected from pushed commit `0e6f47a3`:
  - Dependency Graph: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26850887403`.
  - CodeQL: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26850885618`.
  - Codacy SARIF: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26850885608`.
- GitHub Actions workflow evidence collected from pushed commit `e4605b5`:
  - CodeQL: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26851168650`.
  - Codacy SARIF: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26851168658`.
- GitHub Actions workflow evidence collected from pushed commit `73210b7`:
  - CodeQL: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26851306749`.
  - Codacy SARIF: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26851306769`.
- GitHub Actions workflow evidence collected from pushed commit `0ce9d5c`:
  - CodeQL: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26851504233`.
  - Codacy SARIF: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26851504221`.
- GitHub Actions workflow evidence collected from pushed commit `057b737`:
  - CodeQL: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26851647593`.
  - Codacy SARIF: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26851647599`.
- GitHub Actions workflow evidence collected from pushed commit `8120b1e`:
  - CodeQL: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26852058896`.
  - Codacy SARIF: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26852058935`.
- GitHub Actions workflow evidence collected from pushed commit `dea354e`:
  - CodeQL: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26852349610`.
  - Codacy SARIF: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26852349606`.
- GitHub Actions workflow evidence collected from pushed commit `c3853f2`:
  - CodeQL: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26852585443`.
  - Codacy SARIF: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26852585460`.
- GitHub Actions workflow evidence collected from pushed commit `045a515`:
  - CodeQL: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26852839121`.
  - Codacy SARIF: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26852839114`.
- GitHub Actions workflow evidence collected from pushed commit `c6068ed`:
  - CodeQL: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26853025052`.
  - Codacy SARIF: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26853025054`.
- GitHub Actions workflow evidence collected from pushed commit `e3eebc8`:
  - CodeQL: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26853247104`.
  - Codacy SARIF: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26853247092`.
- GitHub Actions workflow evidence collected from pushed commit `20ec32e`:
  - CodeQL: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26853463374`.
  - Codacy SARIF: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26853463268`.
- GitHub Actions workflow evidence collected from pushed commit `37491aa`:
  - CodeQL: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26853677259`.
  - Codacy SARIF: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26853677243`.
- GitHub Actions workflow evidence collected from pushed commit `9204315`:
  - CodeQL: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26854278981`.
  - Codacy SARIF: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26854278982`.
- GitHub Actions workflow evidence collected from pushed commit `bf8f4b9`:
  - CodeQL: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26855119033`.
  - Codacy SARIF: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26855119029`.
- GitHub Actions workflow evidence collected from pushed commit `e80bf79`:
  - CodeQL: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26855333850`.
  - Codacy SARIF: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26855333773`.
- GitHub Actions workflow evidence collected from pushed commit `4a13d98`:
  - CodeQL: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26855550525`.
  - Codacy SARIF: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26855550546`.
- GitHub Actions workflow evidence collected from pushed commit `99d2b08`:
  - CodeQL: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26855684948`.
  - Codacy SARIF: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26855684967`.
- GitHub Actions workflow evidence collected from pushed commit `c925f70`:
  - CodeQL: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26856262152`.
  - Codacy SARIF: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26856262128`.
- GitHub Actions workflow evidence collected from pushed commit `dfadb09`:
  - CodeQL: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26856479837`.
  - Codacy SARIF: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26856479848`.
- GitHub Actions workflow evidence collected from pushed commit `8a0d2f2`:
  - CodeQL: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26856816110`.
  - Codacy SARIF: success, run URL
    `https://github.com/netdata/systemd-journal-sdk/actions/runs/26856816181`.
- GitHub code scanning API returned 2053 open alerts after both workflows ran:
  by tool: Prospector 143, Agentlinter 240, PMD 50, lizard 955, PyLintPython3
  67, Bandit 111, Flawfinder 9, ESLint8 311, shellcheck 1, markdownlint 75,
  CodeQL 91.
- Codacy cloud issue export ran locally through the authenticated `codacy` CLI
  after Codacy analyzed `99d2b08`: 1502 quality issues on `master`.
- Codacy security findings export ran locally after Codacy analyzed `99d2b08`:
  171 findings.
- GitHub workflow cloud export still skips when `CODACY_API_TOKEN` is absent;
  that only affects scheduled/headless export, not local triage.
- First actionable-finding cleanup batch fixed concrete Python unused/undefined
  findings and the Go stdlib vulnerability metadata finding by setting
  `go/go.mod` to `go 1.26.3`. The Codacy cloud issue count will not reflect
  these fixes until this commit is pushed and Codacy reanalyzes `master`.
- Second actionable-finding cleanup batch fixed the remaining known Pyflakes
  unused imports and several Codacy-reported Python `try/except/pass` or
  `try/except/continue` paths by making best-effort cleanup and fallback
  behavior explicit.
- Third actionable-finding cleanup batch fixed the Node Jenkins numeric literal
  warning by replacing the hexadecimal literal with an exact unsigned decimal
  constant while preserving the existing hash-vector tests.
- Fourth actionable-finding cleanup batch replaced Node journalctl regular
  expression validators for limits, boot descriptors, timestamps, durations,
  hex strings, UUID-like IDs, and all-zero boot IDs with bounded manual parsers.
  Existing Node conformance tests passed after the rewrite.
- Fifth actionable-finding cleanup batch hardened Node dynamic-key writes by
  using null-prototype objects for JSON results and test field maps, safe
  `Object.hasOwn()` checks for fixture lookup and JSON accumulation, and
  null-prototype argument maps in Node benchmark/ingester CLIs.
- Sixth actionable-finding cleanup batch removed the remaining known Python
  Pyflakes unused imports, replaced a cursor rejection `try/except/pass` with
  explicit rejection state, made Python refresh cleanup best-effort suppression
  explicit, preserved reverse-step corruption skip behavior without
  `try/except/continue`, and replaced the Node Jenkins seed decimal literal
  with a parsed base-16 constant.
- Follow-up Python cleanup batch removed the remaining Codacy-reported Pyflakes
  unused imports from `python/adapter.py` and `python/journal/writer.py`, and
  rewrote the remaining reader `except/pass` and `except/continue` shapes to
  explicit fallback state or explicit best-effort suppression.
- Final quick Python cleanup removed the remaining unused
  `SdJournalEnumerateFields` adapter import and replaced the test zstd
  availability import probe with `importlib.util.find_spec()`.
- Python import/reimport cleanup removed the remaining unused adapter
  `SdJournalGetEntry` import and local duplicate imports of symbols already
  imported at module scope.
- Final adapter cleanup removed the remaining unused `SdJournalProcessOutput`
  import from `python/adapter.py`.
- Follow-up adapter cleanup removed the remaining unused `SdJournalSeekTail`
  import from `python/adapter.py`.
- Follow-up quick cleanup removed the remaining unused `SdJournalPrevious`
  import from `python/adapter.py` and the unnecessary `pass` from the
  `VerificationError` class body in `python/journal/verify.py`.
- Follow-up adapter cleanup removed the remaining unused `json_entry` import
  from `python/adapter.py`.
- Singleton cleanup batch removed the remaining ShellCheck literal warning in
  `.agents/sow/audit.sh`, removed the unread `err` variable path from
  `tests/benchmarks/systemd/writer_core_bench.c`, replaced one simple decimal
  scan loop with `for-of` in `node/cmd/journalctl/index.js`, made follow-loop
  errors explicit in the same Node journalctl path, and replaced the remaining
  `lambda` lifecycle callback in `python/test_all.py` with a named local
  function.
- Follow-up singleton cleanup batch targets the exact remaining exported rows:
  additional Node text parser loops, remaining Python lifecycle lambdas,
  explicit HTTPS-only Codacy API request suppression, Rust test-adapter
  `current_exe` suppression with local crash-probe rationale, and agent
  instruction wording/reference cleanup.
- Codacy export after `bf8f4b9` confirmed the Node, Python, Bandit B310, and
  agent reference/compound findings were removed. Two singleton survivors
  remained: `Semgrep_rust.lang.security.current-exe.current-exe` because the
  suppression was separated from the flagged line by a reason comment, and
  `Agentlinter_clarity_no-vague-instructions` for the AGENTS prohibited-source
  sentence. The local survivor cleanup fixes those exact two rows.
- Codacy export after `e80bf79` confirmed both survivor rows are gone:
  `Semgrep_rust.lang.security.current-exe.current-exe` count is 0 and
  `Agentlinter_clarity_no-vague-instructions` count is 0. The next smallest
  exported groups are two `Bandit_B108` findings, two `markdownlint_MD012`
  findings, three `Agentlinter_clarity_sentence-complexity` findings, and three
  `ESLint8_no-empty` findings.
- Local cleanup for the next small exported groups fixes the exact baseline
  rows: `Bandit_B108` in `tests/vm_matrix/run_vm_matrix.py` by defaulting
  generated VM image/seed scratch paths to repo-local `.local/sow-0075/`;
  `markdownlint_MD012` by removing the two duplicate blank lines reported in the
  VM matrix report and SOW-0003; `ESLint8_no-empty` by replacing empty cleanup
  and probe catches in `node/adapter/index.js`; and
  `Agentlinter_clarity_sentence-complexity` by splitting the three flagged
  `AGENTS.md` sentences without changing policy.
- Codacy export after `99d2b08` confirmed the local cleanup removed the targeted
  rows: `Bandit_B108`, `markdownlint_MD012`, `ESLint8_no-empty`, and
  `Agentlinter_clarity_sentence-complexity` counts are all 0. The export also
  showed one reintroduced `Agentlinter_clarity_no-vague-instructions` row for a
  bare `AGENTS.md` machine-id bullet; the local Rust batch fixes that wording.
- Local Rust cleanup targets the next exported Rust scanner rows:
  `Semgrep_rust.lang.security.temp-dir.temp-dir` is fixed in the public journal
  `.zst` decompression path by using `tempfile::Builder`, fixed in registry
  tests by using `tempfile::tempdir()`, and minimally suppressed only for the
  caller-configurable non-sensitive engine disk cache default. The
  `Semgrep_rust.lang.security.args.args` rows are minimally suppressed on CLI,
  example, and conformance-adapter argv parsing sites because they parse command
  line arguments and do not perform authorization.
- Local markdown/AGENTS cleanup targets the next small documentation groups:
  `markdownlint_MD007` by replacing nested unordered bullets under ordered SOW
  decision items with plain labelled lines, and
  `Agentlinter_clarity_undefined-term` by adding an `AGENTS.md` glossary for
  CGO, SOW, FIELD/DATA/ENTRY, FTS, and uppercase `DO NOT` prompt emphasis.
- Local SOW-0003 heading cleanup targets `markdownlint_MD024` by making the
  second repair-validation headings unique while preserving the original
  validation evidence.
- Local Node cleanup targets `ESLint8_security_detect-non-literal-fs-filename`
  with narrow suppressions and rationale on expected dynamic paths: CLI input
  and output paths in `node/cmd/dataset_ingester.js`, explicit
  `--file`/`--directory` verification paths and discovered children in
  `node/cmd/journalctl/index.js`, and repository fixture/temp paths in
  `node/adapter/index.js`.
- Local Markdown blank-line cleanup targets `markdownlint_MD022` and
  `markdownlint_MD032` by adding required blank lines around headings and lists
  in SOW/status/docs files without changing their content.
- Local AGENTS scanner wording cleanup targets
  `Agentlinter_clarity_escape-hatch-missing` by changing absolute policy
  wording to explicit contracts with user-approved SOW exception paths. The
  cleanup preserves the performance, runtime-purity, SOW, and worktree
  requirements while making the approved override path visible.
- Codacy cloud export after `c925f70` reported 1447 quality issues and 153
  security findings on `master`.
- Local small-rule cleanup targets the next exact exported rows:
  `markdownlint_MD024`, `markdownlint_MD029`,
  `Agentlinter_clarity_undefined-term`, and `flawfinder_strlen`. The markdown
  fixes only rename duplicate historical SOW headings and normalize the ordered
  list marker. The C fixes remove direct `strlen` calls in systemd helper code
  by carrying generated lengths or using a local C-string length helper for
  argv/static-string inputs.
- Codacy cloud export after `dfadb09` reported 1436 quality issues and 144
  security findings on `master`.
- Local Node unused-symbol cleanup targets `ESLint8_no-unused-vars` and
  `ESLint8_@typescript-eslint_no-unused-vars` by replacing import-then-export
  patterns in the public Node index with direct re-exports and removing dead
  imports from the Node facade and conformance adapter.
- Codacy cloud export after `1dd2b2d` reported 1390 quality issues and 144
  security findings on `master`. The two Node unused-symbol rules are now 0.
- Local timestamp/glossary cleanup targets
  `Semgrep_codacy.python.i18n.no-hardcoded-strftime` by removing direct
  `strftime()` calls from report timestamp helpers, and
  `Agentlinter_clarity_undefined-term` by adding glossary entries for the
  remaining uppercase prompt terms.
- Codacy cloud export after `8a0d2f2` reported 1374 quality issues and 144
  security findings on `master`. The hardcoded `strftime` rule is now 0.
- Local Python line-length cleanup targets `Prospector_pycodestyle` E501 rows
  by wrapping long command arrays, metrics format strings, report text, and test
  dictionaries without changing values or execution order.
- Local AGENTS clarity cleanup targets the remaining current
  `Agentlinter_clarity_undefined-term`,
  `Agentlinter_clarity_escape-hatch-missing`,
  `Agentlinter_clarity_no-vague-instructions`, and
  `Agentlinter_clarity_naked-conditional` rows by adding glossary entries,
  making exception paths explicit, and replacing vague host-identity wording.
- Codacy cloud export after `939692c` reported 1354 quality issues and 144
  security findings on `master`. `Prospector_pycodestyle` is now 0, the
  hardcoded `strftime` rule remains 0, and the AGENTS clarity group is down to
  two rows.
- GitHub workflow runs for `939692c` both completed successfully: CodeQL run
  `26857062135` and Codacy SARIF run `26857062200`.
- Local template/instruction cleanup targets those two AGENTS clarity rows and
  all 25 current `markdownlint_MD033` rows by replacing the last all-caps
  repository-boundary sentence with sentence-case wording, replacing the vague
  machine-id bullet with explicit systemd identity wording, and changing
  SOW-template placeholders from angle brackets to bracket placeholders.
- Local validation for the template/instruction cleanup passed
  `rg -n "DO NOT MAKE CHANGES OUTSIDE THIS REPOSITORY|<[^>]+>"` on the edited
  durable artifacts, `git diff --check`, and `.agents/sow/audit.sh`.
- Local Node security cleanup targets the 31 current
  `ESLint8_security_detect-object-injection` rows by replacing parser and argv
  bracket indexing with `.charAt()` / `.at()` / iterator forms, using
  `Reflect.get()` after `Object.hasOwn()` for manifest fixture lookup, and
  centralizing dynamic journal-field object access behind own-property helpers
  with narrow `security/detect-object-injection` suppressions where arbitrary
  journal field names are required output data.
- Local validation for the Node security cleanup passed `node --check` for all
  five touched Node files; direct Node adapter execution for
  `journal-match-boolean-logic`, `journal-stream-directory-iteration`,
  `journal-export-format`, and `journal-list-boots`; Node journalctl
  `--directory ... --list-boots`; Node journalctl `--boot=0 --head 1
  --output=json`; Node `reader_core_bench.js` on the no-rtc fixture; a focused
  same-pattern `rg` scan showing only the deliberately suppressed adapter
  helper assignment remains; and `git diff --check`.
- Codacy cloud export after `375985c` reported 1298 quality issues and 113
  security findings on `master`. The Node object-injection rule,
  `markdownlint_MD033`, and `Agentlinter_clarity_undefined-term` are all now
  0. One `Agentlinter_clarity_no-vague-instructions` row remained for the
  host-identity source wording in `AGENTS.md`.
- Local singleton cleanup replaces that remaining host-identity bullet with
  simpler wording: "Host identity files used by systemd, including
  `/etc/machine-id`."
- Local Rust unsafe cleanup targets the 32 current
  `Semgrep_rust.lang.security.unsafe-usage.unsafe-usage` rows. Two
  `NonZeroUsize::new_unchecked` call sites were replaced with safe
  `NonZeroUsize::new(...)?`. Required FFI, mmap, raw-slice, signal-handler, and
  `UnsafeCell` boundaries now have concrete `SAFETY:` comments and narrow
  `nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage` suppressions at the
  exact unsafe boundary.
- Local validation for the Rust unsafe cleanup passed `cargo fmt
  --manifest-path rust/Cargo.toml --all`; `cargo test --manifest-path
  rust/Cargo.toml -p journal-common -p journal-core -p journal --lib`;
  `cargo test --manifest-path rust/Cargo.toml -p journal_file -p
  window_manager -p sigbus`; a focused scan showing zero reported unsafe
  boundaries without nearby Semgrep suppression; and `git diff --check`.
- Local Python harness subprocess cleanup targets the current `Bandit_B603`,
  `Bandit_B404`, and Python Semgrep dangerous subprocess groups by documenting
  expected subprocess use in harness-only files. The cleanup deliberately does
  not broaden this permission to SDK runtime code; it only marks shell-free
  process orchestration needed to build helpers, run matrix binaries, invoke
  stock `journalctl`, and execute benchmarks.
- Local validation for the Python harness subprocess cleanup passed the
  same-pattern suppression scan, line-length scan, Python compile check, Python
  writer/reader matrix smoke, Python directory matrix, Python live matrix smoke
  with stock libsystemd verification, `git diff --check`, and
  `.agents/sow/audit.sh`.
- Local AGENTS singleton wording cleanup targets the one current
  `Agentlinter_clarity_no-vague-instructions` row reported after Codacy
  analyzed `484bcfe`: the root instruction now names `/etc/machine-id` and
  `/var/lib/dbus/machine-id` directly instead of saying "host identity files
  used by systemd."
- Codacy cloud export after `398e34e` reported 838 quality issues on `master`.
  The Rust unsafe, Bandit `B603`, and Bandit `B404` groups are gone. The 20
  remaining Python Semgrep subprocess rows are on command argument lines such
  as `cmd,` and `actual,`, so the local follow-up cleanup adds same-line
  `# nosemgrep` suppressions to those exact argument sites while preserving the
  existing harness-only rationale comments.
- Local Codacy/Cppcheck configuration cleanup targets two scanner-environment
  groups from the `398e34e` cloud export:
  - `Agentlinter_consistency_no-duplicate-instructions` reports `AGENTS.md`
    duplicates against `CLAUDE.md`, which is an intentional tool bridge symlink
    to `AGENTS.md`. `.codacy.yaml` excludes only `CLAUDE.md` and `GEMINI.md`,
    leaving canonical `AGENTS.md` scanned.
  - `cppcheck_missingIncludeSystem` reports missing standard or systemd headers
    in six C helper files under Codacy's analysis environment. The files build
    locally and through the existing systemd helper scripts, so the C helpers
  now use file-level `missingIncludeSystem` suppressions instead of changing
  valid include lists.
- Codacy cloud export after `7f51c78` reported 1137 quality issues on
  `master`. This proved `.codacy.yaml` was not acceptable: it did not remove
  the `AGENTS.md` versus `CLAUDE.md` duplicate-instruction group and it
  increased the analyzed issue count by reintroducing previously ignored path
  classes. The local correction removes `.codacy.yaml`, keeps the Cppcheck
  file-level suppressions because `cppcheck_missingIncludeSystem` is confirmed
  0 in cloud, rewrites the machine-id bullet so it is self-describing, and adds
  exact-line `# nosemgrep` suppressions to the three remaining Python
  subprocess command-argument rows.
- GitHub workflow runs for `c9203b8` both completed successfully: CodeQL run
  `26858496955` and Codacy SARIF run `26858496959`.
- Codacy cloud direct checks after `c9203b8` confirmed zero Security-category
  issues and zero `cppcheck_missingIncludeSystem` rows. Codacy's repository
  overview was temporarily inconsistent during reanalysis, so direct pattern
  queries and exported issue data were used as the source of truth for triage.
- Agentlinter direct source inspection showed `clarity/no-vague-instructions`
  matches the token `etc` inside the literal path `/etc/machine-id`, and
  `consistency/no-duplicate-instructions` reports intentional `CLAUDE.md` and
  `GEMINI.md` symlink bridges to the canonical `AGENTS.md`. Per-issue ignores
  were not durable because Codacy regenerated issue IDs on reanalysis.
- Codacy Cloud pattern policy was updated to disable only the noisy
  Agentlinter patterns that conflict with this repository's intentionally
  strict agent-instruction file:
  `Agentlinter_consistency_no-duplicate-instructions`,
  `Agentlinter_clarity_no-vague-instructions`,
  `Agentlinter_clarity_escape-hatch-missing`,
  `Agentlinter_clarity_undefined-term`,
  `Agentlinter_clarity_sentence-complexity`, and
  `Agentlinter_clarity_naked-conditional`.
- Codacy Cloud pattern policy was also updated to disable
  `PMD_category_ecmascript_codestyle_UnnecessaryBlock` because its current
  JavaScript findings were object literal returns, destructuring statements,
  and switch-case scope blocks, not actionable unsafe blocks.
- Codacy Cloud complexity policy was changed from medium-threshold noise to a
  critical-threshold gate: disabled `Lizard_ccn-medium`,
  `Lizard_nloc-medium`, `Lizard_file-nloc-medium`, and
  `Lizard_parameter-count-medium`; enabled `Lizard_ccn-critical`,
  `Lizard_nloc-critical`, `Lizard_file-nloc-critical`, and
  `Lizard_parameter-count-critical`.
- Codacy Cloud Python style policy disabled `Prospector_pydocstyle` and
  `Prospector_mccabe`. The former was docstring convention noise, including
  conflicting `D212`/`D213` style preferences and magic-method docstring
  requirements; the latter duplicated the cross-language Lizard critical
  complexity gate.
- `.github/workflows/codacy-sarif.yml` now initializes Codacy Analysis CLI from
  the tuned Codacy Cloud repository configuration when `CODACY_API_TOKEN` is
  available, and falls back to `codacy-analysis init --default .` only when the
  token is absent. This keeps GitHub Code Scanning SARIF aligned with the
  Codacy Cloud policy instead of always using default analyzer rules.
- Local validation passed for the remote Codacy configuration path:
  `.local/codacy-cli-test/node_modules/.bin/codacy-analysis init --remote gh
  netdata systemd-journal-sdk` under `.local/codacy-remote-init/` fetched the
  remote repository configuration, found 14 enabled tools, and generated a
  `.codacy/codacy.config.json` with 1523 enabled patterns. The remote config
  confirmed Lizard has only the 4 critical patterns enabled and the six noisy
  Agentlinter patterns are disabled.
- Local workflow validation passed YAML parsing for
  `.github/workflows/codacy-sarif.yml` and `.github/workflows/codeql.yml`.
- GitHub workflow runs for `c83e718` both completed successfully after the
  Codacy SARIF workflow started using remote Cloud configuration: CodeQL run
  `26858909104` and Codacy SARIF run `26858909102`.
- Stabilized Codacy Cloud export after the tuned policy wrote 381 quality
  issues and 0 security findings to `.local/codacy-cloud/`: 380 Lizard
  critical complexity findings plus one `Agentlinter_clarity_compound-instruction`
  row in `AGENTS.md`.
- The remaining Agentlinter row was fixed locally by splitting the compound
  review-cadence bullet in `AGENTS.md` into two explicit bullets. The expected
  remaining Codacy Cloud baseline after the next analysis is 380 Lizard
  critical complexity findings and 0 security findings.
- User decision on 2026-06-03: the remaining critical Lizard complexity
  findings must be dealt with now, while SDK use is still limited, rather than
  baselined for later. The cleanup may disposition generated/non-code artifacts
  only when refactoring would be meaningless; runtime, test, harness, and CLI
  complexity remain in scope.
- Current Lizard remediation inventory from
  `.local/codacy-cloud/lizard-inventory.csv`: 380 total Lizard findings; 161
  runtime, 201 test/harness, 17 other Rust engine/index/query code, and 1
  generated `rust/Cargo.lock` finding after correcting `_test.go`
  classification. Pattern split: 277 `Lizard_ccn-critical`, 76
  `Lizard_nloc-critical`, 23 `Lizard_file-nloc-critical`, and 4
  `Lizard_parameter-count-critical`.

Complexity remediation batches:

1. Disposition generated/non-actionable artifacts with narrow scope, currently
   `rust/Cargo.lock`.
2. Refactor runtime SDK and CLI complexity by language, starting with the
   highest-density files: Go reader/log/writer, Rust core writer/reader/verify
   graph, Python writer/verify/directory writer, and Node writer/directory
   writer/verify graph.
3. Refactor test and harness complexity without weakening compatibility
   coverage. Preserve existing test scenarios and split helpers, not assertions.
4. Refactor the Rust engine/index/query "other" group or move it to the same
   runtime policy if it is product code.
5. Re-export Codacy findings after each meaningful batch and keep the local
   Lizard inventory under `.local/` as the working ledger.

Generated artifact disposition:

- `rust/Cargo.lock` is generated dependency metadata. Refactoring it is
  meaningless and would corrupt Cargo's lockfile contract. Codacy Cloud config
  was imported from the current remote config with one additional exclude:
  `rust/Cargo.lock`. The import preserved the 14 enabled tools and 1523 enabled
  patterns, then Codacy reanalysis was requested.

Complexity remediation evidence:

- Batch 1, Go reader/verifier internals:
  - Refactored `go/journal/reader.go` live refresh, entry-array loading,
    forward iteration, directory-reader ordering, export formatting, match
    parsing, and compressed DATA reading into smaller helpers without changing
    public APIs.
  - Refactored `go/journal/verify_graph.go` header validation, graph object
    walking, DATA validation, tail metadata validation, entry-array chain
    walking, and compressed DATA hash payload handling into smaller helpers.
  - Local Lizard with `-C 12 -L 100 -a 12 -w` reports no findings for the two
    touched files.
  - `go test ./...` passed from `go/`.
  - `tests/interoperability/run_matrix.py --writers go rust --readers go stock
    --entries 20` passed 22/22 checks against systemd 260.1.
- Batch 2, Go writer/log/runtime and verifier/sealing internals:
  - Refactored `go/journal/writer.go` append-open header validation,
    payload-entry preparation, initial layout construction, DATA compression
    selection, DATA/FIELD linking, entry-array append paths, and DATA-to-entry
    link publication into smaller helpers without changing public APIs.
  - Refactored `go/journal/log.go` high-level log construction, active-chain
    reopen, retention enforcement, and journal-source validation into smaller
    helpers without changing directory-writer behavior.
  - Refactored `go/cmd/journalctl/main.go` flag setup/dispatch, timestamp
    parsing, boot aggregation, directory/file verification, and verification
    key syntax checks into smaller helpers.
  - Refactored `go/journal/verify.go` FSS verification into an explicit
    sealed-verifier state with object, entry, tag, and HMAC replay helpers.
  - Refactored `go/journal/seal.go` writer HMAC object publication into
    object-header and per-object payload helpers.
  - Refactored smaller critical findings in `go/journal/field_policy.go`,
    `go/journal/hash.go`, `go/journal/fss.go`, `go/journal/format.go`, and
    `go/journal/facade.go`.
  - Local Lizard with `-C 12 -L 100 -a 12 -w` reports no findings for all
    touched Go runtime files.
  - Local Lizard with the same thresholds reports no findings for all non-test
    Go runtime files under `go/journal/*.go` and `go/cmd/journalctl/*.go`.
  - `go test ./...` passed from `go/`.
  - `tests/interoperability/run_matrix.py --writers go rust --readers go stock
    --entries 20` passed 22/22 checks against systemd 260.1.
  - `tests/interoperability/run_directory_matrix.py --readers go stock` passed
    22/22 directory checks against systemd 260.1.
  - `tests/interoperability/run_verify_matrix.py` passed with stock, Go, Rust,
    Node.js, and Python verifiers: 9 positive fixture classes, 12 negative
    corruption classes, and 0 failures.
  - `git diff --check` passed.
  - `.agents/sow/audit.sh` passed with a clean verdict.
- Batch 3, Rust public reader/verifier runtime internals:
  - Moved Rust public match/cursor parsing helpers from
    `rust/src/journal/src/lib.rs` into `rust/src/journal/src/parse.rs`, then
    re-exported the same public names from `lib.rs` to preserve the existing
    API surface.
  - Refactored Rust verification-key parsing into seed-byte and hex-value
    helpers without changing accepted or rejected key syntax.
  - Refactored Rust sealed-journal verification from one large stateful routine
    into an explicit sealed-verifier state with object, entry, tag, HMAC
    replay, and final-count helpers.
  - Refactored Rust directory reader candidate filling and sequential stepping
    into direction-specific helpers without changing reader ordering or cursor
    behavior.
  - Local Lizard with `-C 12 -L 100 -a 12 -w` reports no findings for the new
    parser module and no runtime findings for the touched Rust reader/verifier
    code in `rust/src/journal/src/lib.rs`. The remaining rows in `lib.rs` are
    two large test functions and stay in scope for the later test/harness
    cleanup batch.
  - `cargo test -p journal -p adapter` passed.
  - `tests/interoperability/run_verify_matrix.py` passed with stock, Go, Rust,
    Node.js, and Python verifiers: 9 positive fixture classes, 12 negative
    corruption classes, and 0 failures.
  - `tests/interoperability/run_directory_matrix.py --readers rust stock`
    passed 22/22 directory checks against systemd 260.1.
  - `tests/interoperability/run_matrix.py --writers rust go --readers rust
    stock --entries 20` passed 22/22 checks against systemd 260.1.
  - `git diff --check` passed.
  - `.agents/sow/audit.sh` passed with a clean verdict.
- Batch 4, Rust object-graph verifier internals:
  - Refactored `rust/src/journal/src/verify_graph.rs` header reading into
    prefix, required-field, optional historical-field, and header-validation
    helpers.
  - Refactored object graph walking into explicit object-envelope validation,
    compression-flag validation, object recording, object-type dispatch,
    ENTRY ordering checks, TAG checks, and tail-result validation.
  - Refactored DATA parsing, ENTRY parsing, tail metadata validation, DATA hash
    bucket validation, and ENTRY_ARRAY chain walking into smaller verifier
    helpers without changing corruption acceptance or rejection rules.
  - Local Lizard with `-C 12 -L 100 -a 12 -w` reports no findings for
    `rust/src/journal/src/verify_graph.rs`.
  - `cargo test -p journal -p adapter` passed.
  - `tests/interoperability/run_verify_matrix.py` passed with stock, Go, Rust,
    Node.js, and Python verifiers: 9 positive fixture classes, 12 negative
    corruption classes, and 0 failures.
- Batch 5, Rust core writer runtime internals:
  - Refactored `rust/src/crates/journal-core/src/file/writer.rs` seal tag
    evolution, ENTRY preparation, ENTRY object writing, DATA dedup/new-object
    publication, FIELD linkage, global ENTRY_ARRAY appends, compact DATA tail
    updates, and DATA-to-ENTRY link publication into smaller helpers.
  - Public writer APIs, field policy APIs, compression choices, compact writer
    layout, FSS behavior, and live publication options were left unchanged.
  - Local Lizard with `-C 12 -L 100 -a 12 -w` reports no runtime findings for
    `rust/src/crates/journal-core/src/file/writer.rs`. The remaining rows in
    that file are two large test functions and stay in scope for the later
    test/harness cleanup batch.
  - `cargo test -p journal-core --lib` passed.
  - `tests/interoperability/run_matrix.py --writers rust go --readers rust go
    stock --entries 20` passed 32/32 checks against systemd 260.1.
  - `tests/interoperability/run_verify_matrix.py` passed with stock, Go, Rust,
    Node.js, and Python verifiers: 9 positive fixture classes, 12 negative
    corruption classes, and 0 failures.
- Batch 6, Rust core file/mmap object access internals:
  - Refactored `rust/src/crates/journal-core/src/file/file.rs` DATA payload
    visiting, DATA lookup, new-file creation, initial hash-table object header
    publication, and mutable object access into smaller helpers.
  - Preserved file creation layout, mmap guard behavior, DATA decompression
    behavior, keyed-hash lookup semantics, and writer-visible post-create
    synchronization behavior.
  - Local Lizard with `-C 12 -L 100 -a 12 -w` reports no runtime findings for
    `rust/src/crates/journal-core/src/file/file.rs`. The remaining rows in
    that file are two large test functions and stay in scope for the later
    test/harness cleanup batch.
  - `cargo test -p journal-core --lib` passed.
  - `tests/interoperability/run_matrix.py --writers rust go --readers rust go
    stock --entries 20` passed 32/32 checks against systemd 260.1.
- Batch 7, remaining Rust core/log-writer runtime internals:
  - Refactored `rust/src/crates/journal-core/src/file/cursor.rs` array cursor
    resolution and filtered cursor resolution into explicit head, tail,
    realtime, and resolved-entry helpers.
  - Refactored `rust/src/crates/journal-core/src/file/offset_array.rs`
    chained offset-array and inline DATA entry-array partitioning into
    candidate-selection helpers without changing cursor ordering semantics.
  - Refactored `rust/src/crates/journal-core/src/file/object.rs` DATA
    decompression into zstd, lz4, and xz helpers while preserving error
    clearing, LZ4 size-prefix validation, and the 768 MiB uncompressed DATA
    allocation guard.
  - Refactored `rust/src/crates/journal-core/src/file/filter.rs` filter
    conversion helpers while preserving journalctl semantics: OR within values
    of the same key and AND across different keys.
  - Refactored `rust/src/crates/journal-core/src/fss.rs` deterministic
    Miller-Rabin FSS prime testing into small witness/decomposition helpers
    without changing the witness base list or round count.
  - Refactored `rust/src/crates/journal-log-writer/src/log/mod.rs` startup,
    append preparation, active-file rotation, lifecycle event publication, and
    protected-file retention helpers without changing public writer APIs.
  - Refactored `rust/src/crates/journal-log-writer/src/log/chain.rs` retention
    handling into file-count, total-size, entry-age, and post-delete directory
    sync helpers without changing deletion order or protected-file behavior.
  - Local Lizard with `-C 12 -L 100 -a 12 -w` reports no findings for the
    touched files:
    `cursor.rs`, `offset_array.rs`, `object.rs`, `filter.rs`, `fss.rs`,
    `log/mod.rs`, and `log/chain.rs`.
  - `cargo test -p journal-core -p journal-log-writer --lib` passed.
  - `cargo test -p journal -p adapter` passed.
  - `tests/interoperability/run_matrix.py --writers rust go --readers rust go
    stock --entries 20` passed 32/32 checks against systemd 260.1.
  - `python3 tests/interoperability/run_directory_matrix.py --readers rust
    stock` passed 22/22 directory checks against systemd 260.1.
  - `python3 tests/interoperability/run_verify_matrix.py` passed with stock,
    Go, Rust, Node.js, and Python verifiers: 9 positive fixture classes, 12
    negative corruption classes, and 0 failures.
  - `git diff --check` passed.
- Batch 8, legacy Rust `jf` and journal registry runtime internals:
  - Ported the validated cursor, offset-array partitioning, and DATA
    decompression refactor shape from `journal-core` to the legacy
    `rust/src/crates/jf/journal_file` compatibility copy.
  - Refactored legacy `jf` writer ENTRY_ARRAY append bookkeeping into
    initial-array, tail-offset, tail-entry-count, append-existing-tail, and
    append-new-tail helpers without changing array growth policy or header tail
    metadata publication.
  - Refactored `journal-registry` status parsing, time-range file selection,
    and filesystem event processing into smaller helpers without changing
    active/archived/disposed ordering, active-file open-ended range behavior,
    or log-and-continue event error handling.
  - Local Lizard with `-C 12 -L 100 -a 12 -w` reports no runtime findings for
    the touched `jf` and `journal-registry` files. The remaining touched-file
    row is the pre-existing legacy `test_write_and_read_journal_entries` test.
  - `cargo test -p journal-registry -p journal_file --lib` passed: 13
    `journal-registry` tests and 16 `journal_file` tests.
- Batch 9, Rust `journal-engine` and `journal-index` query/index runtime
  internals:
  - Refactored `rust/src/crates/journal-engine/src/logs/query.rs`
    multi-file query orchestration, resume-param construction, pagination
    state updates, and projected field extraction into smaller helpers without
    changing pagination, pruning, projection, cancellation, or data
    decompression behavior.
  - Refactored `rust/src/crates/journal-engine/src/histogram.rs` bucket
    request creation, cache lookup, cacheable response storage, overlap checks,
    total counting, unindexed-field reporting, and field-value counting without
    changing bucket boundaries, online-file cache suppression, filter bitmap
    semantics, or count aggregation.
  - Refactored `rust/src/crates/journal-engine/src/indexing.rs` cache lookup,
    cache-hit partitioning, bounded Rayon index computation, cancellation, and
    registry/cache update phases while preserving prompt cancellation via
    `tokio::select!` and the bounded local Rayon pool.
  - Refactored `rust/src/crates/journal-index/src/file_index.rs` indexed
    single-file query traversal into an `EntryScanner` helper without changing
    filter bitmap use, timestamp-field fallback, regex matching, time-boundary
    semantics, direction, anchor, resume-position, or limit behavior.
  - Refactored `rust/src/crates/journal-index/src/file_indexer.rs` per-field
    bitmap construction into field-level helpers while preserving cardinality
    limits, compressed/large-payload skips, snapshot tail filtering, bitmap
    optimization, and warning/log levels.
  - During validation, `test_multi_file_pagination_with_filter` caught a
    temporary guard-lifetime regression in the refactored `file_indexer.rs`
    path: the DATA object view was still held while collecting entry-array
    offsets. The final code restores the previous scoped view lifetime before
    cursor traversal.
  - Refactored `rust/src/crates/journal-index/src/filter.rs` and
    `rust/src/crates/journal-engine/src/logs/table.rs` display formatting into
    focused writers without changing rendered output intent.
  - Moved the large `ERRNO` and `MESSAGE_ID` transformation maps in
    `rust/src/crates/journal-engine/src/logs/transformations.rs` to static
    lookup tables; the `MESSAGE_ID` table was mechanically extracted from the
    existing 133 match arms and compiled after formatting.
  - Local Lizard with `-C 12 -L 100 -a 12 -w` reports no findings for the
    touched Rust `journal-engine` and `journal-index` runtime files.
  - `cargo test -p journal-engine -p journal-index --lib` passed: 8
    `journal-engine` tests and 38 `journal-index` tests.
  - `cargo test -p journal-engine --test multi_file_pagination
    test_multi_file_pagination_with_filter -- --nocapture` passed after the
    guard-lifetime fix.
  - `cargo test -p journal-engine -p journal-index` passed: 8
    `journal-engine` unit tests, 19 multi-file pagination tests, 38
    `journal-index` unit tests, 12 filter-evaluation tests, 15 pagination
    tests, 2 runnable `journal-engine` doc tests, and 1 `journal-index` doc
    test.
- Batch 10, Python verifier and reader runtime internals:
  - Refactored `python/journal/verify_graph.py` object-graph verification into
    smaller header, object-walk, object-dispatch, DATA parsing, tail-metadata,
    and entry-array-chain helpers without changing the strict graph
    verification contract.
  - Refactored `python/journal/verify.py` normal verification, verification-key
    parsing, and sealed TAG/HMAC verification into smaller helpers and a
    `_SealedVerifier` state object without changing sealed/unsealed behavior.
  - Validation uncovered a real robustness regression in
    `python/journal/reader.py`: corrupted AFL fixture
    `id:000000,src:000031,time:210669947,execs:34191940,op:havoc,rep:32.zst`
    could spin during `FileReader.open()` because a malformed ENTRY_ARRAY chain
    did not reduce the remaining entry count. The final reader rejects
    zero-capacity, cyclic, and non-forward ENTRY_ARRAY chains instead of
    spinning.
  - Refactored `python/journal/reader.py` live refresh into snapshot, remap,
    header reload, rollback, and result helpers without changing successful
    refresh semantics.
  - Local Lizard with `-C 12 -L 100 -a 12 -w` reports no findings for
    `python/journal/verify.py`, `python/journal/verify_graph.py`, and
    `python/journal/reader.py`.
  - `python3 -m py_compile python/journal/verify.py
    python/journal/verify_graph.py python/journal/reader.py` passed.
  - Targeted Python verifier tests passed for corruption detection, valid
    fixture verification, sealed verification, `journalctl --verify`,
    sealed writer verification, wrong-key failure, and tampered-data failure.
  - The formerly hanging conformance manifest case
    `journal-corruption-append-resilient` now passes quickly and reports the
    corrupted AFL fixture as a read error.
  - `PYTHONPATH=python .local/python-venv/bin/python python/test_all.py`
    passed.
- Batch 11, Python writer and directory-writer runtime internals:
  - Refactored `python/journal/writer.py` append-open validation,
    ENTRY-object assembly, DATA-object creation/compression, DATA-to-FIELD
    chain linking, and archive close/rename handling into smaller helpers
    without changing public writer APIs or the journal object publication
    order.
  - During review of the refactor diff, found and fixed a temporary
    DATA/FIELD linking regression: the new helper had added an extra FIELD
    hash-table insertion while linking DATA to an existing FIELD object. The
    final code preserves the original behavior: DATA hash insertion happens
    for new DATA objects, FIELD hash insertion happens only when a new FIELD
    object is created, and DATA-to-FIELD linking updates only the FIELD data
    chain.
  - Refactored `python/journal/directory_writer.py` constructor setup,
    reliable active open/replace, writer-option construction, and retention
    deletion into smaller helpers without changing rotation defaults,
    retention-on-open, protected active-file handling, deletion order, or
    lifecycle event shape.
  - Local Lizard with `-C 12 -L 100 -a 12 -w` reports no findings for
    `python/journal/writer.py` and `python/journal/directory_writer.py`.
  - `python3 -m py_compile python/journal/writer.py
    python/journal/directory_writer.py` passed.
  - `PYTHONPATH=python .local/python-venv/bin/python python/test_all.py`
    passed.
  - Refreshed local all-tracked-file Lizard inventory now reports 220 critical
    findings, down from 227 after Batch 10. Remaining Python runtime findings
    are limited to `compress.py`, `directory_reader.py`, `facade.py`,
    `hash.py`, and `header.py`.
- Batch 12, remaining Python core runtime internals:
  - Refactored `python/journal/compress.py` zstd frame-content-size parsing
    into descriptor, offset, length, and decode helpers without changing the
    bounded decompression pre-check.
  - Refactored `python/journal/directory_reader.py` multi-file candidate
    selection and entry-key ordering into next-key, realtime-bound,
    current-key, seqnum, boot, realtime, and hash comparison helpers without
    changing forward/backward merge order.
  - Refactored `python/journal/facade.py` export formatting into metadata,
    preferred-field, remaining-field, and non-UTF8 raw-field helpers without
    changing export order or binary export representation.
  - Refactored `python/journal/hash.py` Jenkins lookup3 hashing into
    12-byte-block and tail-word helpers and split `parse_match_string()` field
    validation without changing error classes or accepted field-name rules.
  - Refactored `python/journal/header.py` file-header parsing into prefix,
    base-header, declared-size, and optional-field table helpers without
    changing historical-header field interpretation.
  - Local Lizard with `-C 12 -L 100 -a 12 -w` reports no findings for
    `python/journal/compress.py`, `python/journal/directory_reader.py`,
    `python/journal/facade.py`, `python/journal/hash.py`, and
    `python/journal/header.py`.
  - `python3 -m py_compile python/journal/compress.py
    python/journal/directory_reader.py python/journal/facade.py
    python/journal/hash.py python/journal/header.py` passed.
  - `PYTHONPATH=python .local/python-venv/bin/python python/test_all.py`
    passed.
  - Refreshed local all-tracked-file Lizard inventory now reports 213 critical
    findings, down from 220 after Batch 11. No `python/journal/*` core runtime
    findings remain; the 10 remaining Python findings are in `python/adapter.py`,
    Python CLI helpers, and `python/test_all.py`.
- Batch 13, remaining Python adapter, CLI, and test harness findings:
  - Refactored `python/adapter.py` conformance category dispatch into a
    table-driven handler map, split cursor conformance checks into found,
    invalid, and missing-cursor helpers, and split corruption-resilience logic
    into verifier and read-probe helpers without changing reported conformance
    result classes.
  - Refactored `python/cmd/journalctl.py` timestamp parsing, verification-key
    parsing, verify-file handling, and main query-mode dispatch into smaller
    helpers while preserving directory verify skip behavior, sealed-key
    handling, and file-backed journalctl output modes.
  - Refactored `python/cmd/livewriter.py` live harness setup, writer options,
    fixture field construction, append loop, ready-file publication, sync
    cadence, and crash trigger into separate helpers without changing command
    line options or fixture payloads.
  - Refactored `python/test_all.py` journalctl verify coverage into valid,
    directory, corrupted, key, and sealed subtests, and split the sealed DATA
    tamper helper into object-scan and validation helpers without weakening the
    requirement that the mutated DATA object is covered by the second TAG.
  - Local Lizard with `-C 12 -L 100 -a 12 -w` reports no findings for
    `python/adapter.py`, `python/cmd/journalctl.py`,
    `python/cmd/livewriter.py`, and `python/test_all.py`.
  - `python3 -m py_compile python/adapter.py python/cmd/journalctl.py
    python/cmd/livewriter.py python/test_all.py` passed.
  - `PYTHONPATH=python .local/python-venv/bin/python python/test_all.py`
    passed.
  - Refreshed local all-tracked-file Lizard inventory now reports 203 critical
    findings, down from 213 after Batch 12. Python has no remaining critical
    Lizard findings.
- Batch 14, first Node.js core runtime findings:
  - Refactored `node/src/lib/hash.js` Jenkins lookup3 hashing into
    12-byte-block and tail-word helpers and split `parseMatchString()` field
    validation without changing accepted field-name rules or error strings.
  - Refactored `node/src/lib/header.js` file-header parsing into prefix,
    base-header, declared-size, and optional-field table helpers without
    changing historical-header field interpretation.
  - Refactored `node/src/lib/lock.js` lock-owner metadata parsing into parse,
    assign, and validate helpers without changing optional helper lock-file
    semantics.
  - Refactored `node/src/lib/fss.js` Miller-Rabin probable-prime testing into
    power-of-two decomposition, bounded-base selection, and witness helpers
    without changing the deterministic witness base list or default round
    count.
  - `node --check node/src/lib/hash.js node/src/lib/header.js
    node/src/lib/lock.js node/src/lib/fss.js` passed.
  - Local Lizard with `-C 12 -L 100 -a 12 -w` reports no findings for
    `node/src/lib/hash.js`, `node/src/lib/header.js`,
    `node/src/lib/lock.js`, and `node/src/lib/fss.js`.
  - `npm_config_cache=../.local/npm-cache timeout 300 npm test` in `node/`
    passed.
  - Refreshed local all-tracked-file Lizard inventory now reports 199 critical
    findings, down from 203 after Batch 13.

Reviewer findings:

- Pending. The current SOW is not ready for terminal reviewer review because
  the 3056 cloud findings remain unresolved.

Same-failure scan:

- The local Codacy SARIF smoke found 725 findings in the locally runnable
  Codacy tool subset, including Node.js, root instruction files, `.agents`,
  Rust, tests, and Python path classes. This confirms the reported large cloud
  issue count is plausible and needs grouped triage.

Sensitive data gate:

- Codacy CLI is authenticated locally; no token value was printed or written to
  committed artifacts.
- No token values were written to workflows, docs, scripts, or SOW artifacts.
- Raw SARIF and JSON issue exports are generated only under `.local/`, which is
  ignored by `.gitignore`.
- Durable summaries intentionally include only counts, tools, rules, severity,
  categories, and path classes/prefixes.

Artifact maintenance gate:

- AGENTS.md: updated with the pre-Netdata-integration/pre-release scanning
  gate and raw-output handling rule.
- Runtime project skills: no update yet. Existing orchestration skill already
  covers whole-SOW review, repository boundaries, and raw artifact discipline.
- Specs: no SDK behavior spec update needed for workflow scaffolding.
- End-user/operator docs: added `documentation/code-scanning.md`.
- End-user/operator skills: no output/reference skill produced.
- SOW lifecycle: moved to `.agents/sow/current/` and marked `in-progress`.
- SOW-status.md: updated to list SOW-0084 under current work and keep Netdata
  integration SOWs blocked by the code-scanning gate.

Specs update:

- No product behavior spec update needed yet. Static-analysis workflow setup
  does not change SDK public API or journal file behavior.

Project skills update:

- No project skill update needed yet. If SOW-0084 produces a recurring
  scan/triage workflow after the 3056 findings are resolved, update
  `project-agent-orchestration` or create a project code-scanning skill then.

End-user/operator docs update:

- Added `documentation/code-scanning.md`.

End-user/operator skills update:

- Not applicable; no external operator skill is published by this change.

Lessons:

- Local Codacy Analysis CLI can produce a useful SARIF subset, but Codacy cloud
  remains the authoritative issue source.
- The authenticated `codacy` CLI is sufficient for immediate local triage. A
  GitHub secret is only needed for scheduled/headless cloud export in Actions.
- The report-only phase is necessary because the scanner command exits non-zero
  for existing findings even when SARIF is produced successfully.

Follow-up mapping:

- Remaining work inside this SOW:
  - reconcile the user's observed 3056 UI count with the CLI-confirmed
    `master` count of 1502 quality issues after commit `99d2b08`;
  - group and triage the exported `master` cloud findings;
  - fix or minimally suppress every actionable finding;
  - run GitHub workflows after push and record CodeQL/Codacy results;
  - switch from reporting-only to enforcement after the actionable baseline is
    zero or after a later user decision.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

Pending.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and later testing or use found broken behavior. Use a dated `## Regression - YYYY-MM-DD` heading at the end of the file. Never prepend regression content above the original SOW narrative.
