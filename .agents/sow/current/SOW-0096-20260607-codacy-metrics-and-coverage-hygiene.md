# SOW-0096 - Codacy Metrics And Coverage Hygiene

## Status

Status: in-progress

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: started after the user reported that Codacy coverage includes tests
and repository-level complexity and duplication remain too high.

## Requirements

### Purpose

Keep Codacy useful as a high-signal pre-integration gate for this SDK, with
coverage, complexity, and duplication metrics that reflect production Rust and
Go code rather than tests, generated artifacts, or metric noise.

### User Request

The user requested:

1. Exclude tests from Codacy coverage reporting.
2. Investigate Codacy repository-level complexity (`46%`) and duplication
   (`31%`), focusing mainly on Rust and Go.
3. Perform file-by-file analysis and consult Codacy for every Rust/Go file to
   decide whether the complexity, Rust complexity, and duplication indicators
   are reasonable.

### Assistant Understanding

Facts:

- Codacy Cloud for `netdata/systemd-journal-sdk` on head
  `cc6e2e6948bf3e57200a7671ddfab4f4310110b9` reports:
  - `issuesCount = 0`;
  - `coveragePercentage = 66`;
  - `complexFilesPercentage = 46`;
  - `duplicationPercentage = 31`.
- The repository has a committed coverage workflow at
  `.github/workflows/coverage.yml`.
- Coverage generation scripts live under `tests/coverage/`.
- Node.js coverage already filters to `node/src` and excludes test paths.
- Python coverage uses `--source=journal`, so it is already constrained to the
  Python package source.
- Go coverage uses `go test -covermode=atomic -coverpkg=./...`, then uploads
  the resulting report.
- Rust coverage uses `cargo llvm-cov --workspace --lcov`, then uploads the
  resulting report.
- Codacy's official documentation states that coverage-only exclusions must be
  handled in the coverage generator reports, not Codacy configuration.
- Codacy's official documentation exposes file-level metrics through the
  `listFiles` API, including file path, grade, total issues, complexity,
  coverage, and duplication.

Inferences:

- The coverage fix should modify generated coverage reports so test files do
  not reach Codacy, instead of relying on Codacy path exclusions.
- The Rust/Go complexity and duplication audit needs the Codacy file metrics
  API, because `codacy issues` is already clean and no longer lists the metric
  offenders.
- The audit should not immediately suppress or refactor large areas. The first
  deliverable is a file-by-file evidence report that classifies each Rust/Go
  metric as actionable, acceptable, generated/test/vendor noise, or needing a
  follow-up SOW.

Unknowns:

- Whether Codacy's aggregate complexity and duplication percentages are driven
  mostly by test/helper files, generated files, legacy compatibility code,
  intentionally table-like code, or production hot paths.
- Whether Codacy's Rust complexity metric is materially different from local
  Lizard or Rust source structure in a way that needs Codacy-side configuration
  rather than code changes.

### Acceptance Criteria

- Codacy coverage upload reports no longer include test files for Rust and Go,
  and the SOW records the exact filtering mechanism.
- Existing Node.js and Python coverage exclusions are verified and left
  unchanged unless evidence shows they still leak test files.
- A reproducible, repository-local tool or command sequence retrieves Codacy
  file metrics for Rust and Go through the Codacy API without writing tokens or
  raw credentials to durable artifacts.
- A durable, sanitized Rust/Go file-level metrics report exists under
  `.agents/sow/specs/` or another committed documentation path, with per-file
  classification for complexity and duplication reasonableness.
- Raw API responses, if retained, live only under `.local/`.
- The SOW records which files need code refactors, which need metric
  configuration/exclusion, and which are acceptable despite high metrics.
- Local validation passes: coverage scripts generate reports, report filters
  remove test files, `git diff --check`, and `.agents/sow/audit.sh`.
- Remote validation after push shows Codacy on the pushed head with test-file
  coverage exclusions applied and no new CodeQL/Codacy issues.

## Analysis

Sources checked:

- `AGENTS.md` and SOW rules.
- `.agents/skills/project-agent-orchestration/SKILL.md`.
- `.github/workflows/coverage.yml`.
- `tests/coverage/run_go_coverage.sh`.
- `tests/coverage/run_rust_coverage.sh`.
- `tests/coverage/run_python_coverage.sh`.
- `tests/coverage/run_node_coverage.sh`.
- `tests/coverage/upload_codacy_coverage.sh`.
- Codacy Cloud CLI repository query for `netdata/systemd-journal-sdk`.
- Codacy docs, "Obtaining code quality metrics for files":
  `https://docs.codacy.com/codacy-api/examples/obtaining-code-quality-metrics-for-files/`.
- Codacy docs, "Codacy configuration file":
  `https://codacy.zendesk.com/hc/en-us/articles/23113835155100-Codacy-configuration-file`.
- Codacy docs, "Which metrics does Codacy calculate?":
  `https://docs.codacy.com/faq/code-analysis/which-metrics-does-codacy-calculate/`.

Current state:

- Codacy Cloud reports zero issues and zero open findings, but poor aggregate
  metric percentages.
- Coverage workflow uses separate Go, Rust, Python, and Node.js jobs and then
  uploads partial coverage reports to Codacy.
- The coverage upload job uses `CODACY_API_TOKEN` from GitHub secrets and does
  not print the token.
- There is no committed `.codacy/` configuration file in this repository.

Risks:

- Excluding tests from coverage incorrectly could hide source files or make
  coverage artificially optimistic.
- Refactoring high-complexity files blindly could damage hot paths or
  compatibility logic.
- Broad Codacy exclusions could make future findings invisible and reduce the
  value of the gate.
- Codacy file-level API responses may contain paths and metrics only, but raw
  responses still remain scratch evidence and should stay under `.local/`.
- Coverage changes require a remote Codacy run to fully validate the dashboard
  effect.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Codacy coverage is only as accurate as the reports uploaded by CI. If reports
  contain test files, Codacy will include those files in coverage calculations.
  Codacy documentation explicitly places coverage-only exclusions in the
  report-generation tools.
- Codacy's complexity and duplication dashboard percentages are aggregate file
  metrics, not issue lists. Since `issuesCount = 0`, the next step is to query
  file-level metrics through Codacy's `listFiles` API and classify Rust/Go
  files directly.

Evidence reviewed:

- `.github/workflows/coverage.yml`: coverage artifacts are produced per
  language and uploaded by `tests/coverage/upload_codacy_coverage.sh`.
- `tests/coverage/run_go_coverage.sh`: Go report currently covers `./...` and
  normalizes all paths under `go/`.
- `tests/coverage/run_rust_coverage.sh`: Rust report currently includes the
  whole Rust workspace.
- `tests/coverage/run_node_coverage.sh`: Node.js report filters to
  `node/src` and excludes `node/test`.
- `tests/coverage/run_python_coverage.sh`: Python report uses
  `--source=journal`.
- Codacy Cloud repository query: head
  `cc6e2e6948bf3e57200a7671ddfab4f4310110b9`, `issuesCount = 0`,
  `coveragePercentage = 66`, `complexFilesPercentage = 46`,
  `duplicationPercentage = 31`.
- Official Codacy docs state that file metrics are available through the
  `listFiles` API and that coverage-only exclusions must be handled in coverage
  reports.

Affected contracts and surfaces:

- GitHub Coverage workflow behavior.
- Coverage reports uploaded to Codacy for Go and Rust.
- Codacy dashboard metrics used as the pre-Netdata-integration gate.
- SOW/spec documentation around static-analysis health.
- Potential follow-up SOWs for Rust/Go code refactors or Codacy metric
  configuration.

Existing patterns to reuse:

- Existing `tests/coverage/lib.sh` helper style.
- `.local/` for raw API responses and generated scratch reports.
- SOW-0084 remote validation discipline for CodeQL/Codacy gates.
- Explicit-path staging and no-token durable artifact policy.

Risk and blast radius:

- Medium CI risk: coverage scripts may fail if path filtering accidentally
  produces malformed reports.
- Medium metric risk: excluding too much may improve numbers without improving
  code quality.
- High engineering risk if high-complexity parser/hot-path files are refactored
  before classification and benchmarks.
- Low runtime risk for coverage-report-only changes.

Sensitive data handling plan:

- Do not print, copy, or commit Codacy tokens or credential files.
- Use the existing authenticated Codacy CLI or environment variables only in
  commands that do not echo token values.
- Store raw Codacy API responses only under `.local/`.
- Durable reports may include file paths and numeric metrics, but not API
  tokens, headers, private URLs, or raw credential material.

Implementation plan:

1. Add report-level test exclusion for Go and Rust coverage generation.
2. Add or update coverage validation helpers to prove test paths are absent
   from uploaded reports.
3. Implement a local script or command sequence to query Codacy `listFiles`
   metrics for `go/` and `rust/`, with pagination.
4. Generate a sanitized committed report that classifies Rust/Go high
   complexity and duplication files file by file.
5. Validate locally, then push and verify CodeQL/Codacy/Coverage remotely.

Validation plan:

- Run changed coverage scripts locally where practical.
- Search generated coverage reports for `*_test.go`, `/test/`, `tests/`, and
  Rust test-only paths.
- Query Codacy file metrics and compare aggregate offenders with local file
  inventory.
- `git diff --check`.
- `.agents/sow/audit.sh`.
- Remote GitHub Actions and Codacy Cloud validation after push.

Artifact impact plan:

- AGENTS.md: no expected update unless a durable project-wide Codacy metric
  policy is needed.
- Runtime project skills: no expected update unless Codacy metric analysis
  becomes a repeated workflow.
- Specs: expected update or new spec/report under `.agents/sow/specs/` for
  Rust/Go Codacy metrics classification.
- End-user/operator docs: likely unaffected; this is CI/quality gate work.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: this SOW records the new gate work and any follow-up SOWs.
- SOW-status.md: update while active and on close.

Open-source reference evidence:

- No local mirrored OSS repositories were checked. This SOW concerns Codacy
  metrics and repository-local CI behavior, so official Codacy documentation
  and local repository code are the relevant references.

Open decisions:

- None at SOW start. The user has already directed that tests be excluded from
  coverage and that Rust/Go complexity and duplication be analyzed file by
  file before deciding deeper remediation.

## Implications And Decisions

- User decision: tests must be excluded from Codacy coverage reporting.
- User decision: Rust and Go are the priority for complexity and duplication
  analysis.
- User decision: Codacy should be consulted file by file before judging whether
  the complexity and duplication indicators are reasonable.

## Plan

1. Fix coverage report generation so tests are not uploaded to Codacy.
2. Build the Codacy file-metrics extraction path for Rust and Go.
3. Produce the Rust/Go file-by-file complexity/duplication classification.
4. Validate locally and remotely.

## Delegation Plan

Implementer:

- Local implementation by the project manager, per current project routing.

Reviewers:

- Run the current reviewer pool after the SOW implementation and local
  validation are complete, unless the user changes the review cadence.

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

- If Codacy file metrics cannot be fetched through available credentials, record
  the blocker and use only local metrics as provisional evidence, but do not
  close the SOW until Codacy-backed evidence exists or the user waives it.

## Execution Log

### 2026-06-07

- Created the SOW and recorded the coverage/Codacy metric requirements.
- Added shared coverage report filters in `tests/coverage/lib.sh`:
  - Go upload reports remove `internal/testcmd/`, `tests/`, `test/`,
    `testdata/`, and `*_test.go`.
  - Rust LCOV upload reports remove `internal/testcmd/`, `tests/`, `test/`,
    `tests.rs`, `testdata/`, `*_test.rs`, and `examples/`.
  - Both formats now have `assert_no_coverage_test_paths` guards.
- Updated `tests/coverage/run_go_coverage.sh`:
  - raw Go coverage remains available under `.local/`;
  - Codacy upload report is normalized and filtered to repository-relative
    `go/` paths;
  - local human summary is generated from a filtered `./go/` summary profile,
    not from the raw report that contains `internal/testcmd`.
- Updated `tests/coverage/run_rust_coverage.sh`:
  - raw LCOV remains available under `.local/`;
  - Codacy upload report is normalized and filtered to repository-relative
    `rust/` paths.
- Updated `tests/coverage/README.md` to document coverage-test exclusion as a
  producer/report responsibility.
- Added `tests/code_scanning/export_codacy_file_metrics.js` to query Codacy
  `listFiles` file metrics for `go/` and `rust/` through the installed Codacy
  Cloud CLI client. The script writes sanitized file/path/metric JSON only.
- Added `tests/code_scanning/summarize_codacy_file_metrics.py` to join Codacy
  file metrics with local Lizard max-function CCN and classify each Rust/Go
  file.
- Extended `tests/code_scanning/test_summarize_findings.py` with unit tests for
  the new classification logic and caught a real `_test.rs` classifier drift
  during validation.
- Generated `.agents/sow/specs/codacy-rust-go-metrics-audit.md` from Codacy
  file metrics and local Lizard data.
- Created follow-up SOWs for real metric debt that should not be hidden by this
  coverage/audit SOW:
  - `.agents/sow/pending/SOW-0097-20260607-go-codacy-metric-debt-refactor.md`.
  - `.agents/sow/pending/SOW-0098-20260607-rust-legacy-core-duplication-debt.md`.

Codacy Rust/Go file metrics evidence:

- `tests/code_scanning/export_codacy_file_metrics.js --output .local/codacy/file-metrics-rust-go.validation.json --search go/ --search rust/`
  returned `217` Rust/Go files for branch `master`.
- Top Codacy complexity files:
  - `go/journal/netdata.go`: complexity `870`, duplication `0`, coverage
    `72.32`.
  - `go/journal/explorer.go`: complexity `763`, duplication `111`, coverage
    `78.46`.
  - `go/cmd/journalctl/main.go`: complexity `304`, duplication `71`,
    coverage `40.52`.
  - `go/journal/verify_graph.go`: complexity `276`, duplication `16`,
    coverage `65.74`.
  - `go/journal/directory_reader.go`: complexity `263`, duplication `101`,
    coverage `63.11`.
- Local Lizard on tracked Rust/Go source files found no function above max CCN
  `12`. This means the largest Go complexity offenders are file
  ownership/size pressure, not single-function complexity failures.
- Top production duplication is real Rust legacy/core overlap:
  - `rust/src/crates/jf/journal_file/src/file.rs`: duplication `686`.
  - `rust/src/crates/journal-core/src/file/offset_array.rs`: duplication
    `662`.
  - `rust/src/crates/jf/journal_file/src/offset_array.rs`: duplication `600`.
  - `rust/src/crates/journal-core/src/file/file.rs`: duplication `491`.
  - `rust/src/crates/jf/journal_file/src/journal_file.rs`: duplication `427`.
- Largest overall duplication contributors are tests/harnesses and now remain
  classified separately from production coverage decisions.

## Validation

Acceptance criteria evidence:

- Go and Rust coverage upload reports now strip test/test-harness paths before
  Codacy upload.
- Python and Node.js existing coverage reports were verified and still contain
  no test-like paths:
  - Python coverage uses `--source=journal`.
  - Node.js coverage uses `--all node/src` and explicit test/vendor filters.
- Codacy file metrics were queried through a repository-local script, with raw
  API responses stored only under `.local/`.
- The committed file-by-file Rust/Go metrics audit is
  `.agents/sow/specs/codacy-rust-go-metrics-audit.md`.
- Follow-up SOWs map real production metric debt:
  - SOW-0097 for Go file-size/ownership and duplication refactor decisions.
  - SOW-0098 for Rust `jf`/`journal-core` duplication refactor decisions.

Tests or equivalent validation:

- `python3 -m pytest tests/code_scanning/test_summarize_findings.py`: `13`
  tests passed.
- `node --check tests/code_scanning/export_codacy_file_metrics.js`: passed.
- `python3 -m py_compile tests/code_scanning/summarize_codacy_file_metrics.py tests/code_scanning/test_summarize_findings.py`: passed.
- `tests/coverage/run_go_coverage.sh .local/coverage/go-sow96`: passed.
  Final upload report has `32` files and `0` test-like paths; filtered local
  summary reports `71.2%` statement coverage.
- `tests/coverage/run_rust_coverage.sh .local/coverage/rust-sow96`: passed.
  Final LCOV upload report has `83` files and `0` test-like paths.
- `env PATH="$PWD/.local/coverage-python-venv/bin:$PATH" tests/coverage/run_python_coverage.sh .local/coverage/python-sow96`:
  passed. Final Cobertura report has `25` files and `0` test-like paths.
- `tests/coverage/run_node_coverage.sh .local/coverage/node-sow96`: passed.
  Final LCOV report has `23` files and `0` test-like paths.
- Post-review-fix validation:
  - `python3 -m pytest tests/code_scanning/test_summarize_findings.py`: `13`
    tests passed.
  - `node --check tests/code_scanning/export_codacy_file_metrics.js`: passed.
  - `python3 -m py_compile tests/code_scanning/summarize_codacy_file_metrics.py tests/code_scanning/test_summarize_findings.py`:
    passed.
  - `tests/coverage/run_go_coverage.sh .local/coverage/go-sow96-fix`: passed;
    final upload report has `32` files and `0` test-like paths.
  - `tests/coverage/run_rust_coverage.sh .local/coverage/rust-sow96-fix`:
    passed; final upload report has `83` files, `0` test-like paths, `83`
    `SF` records, and `83` `end_of_record` markers.
- `tests/code_scanning/export_codacy_file_metrics.js --output .local/codacy/file-metrics-rust-go.validation.json --search go/ --search rust/`:
  passed and returned `217` files.
- `python3 tests/code_scanning/summarize_codacy_file_metrics.py --metrics .local/codacy/file-metrics-rust-go.validation.json --lizard-csv .local/codacy/lizard-rust-go.csv --markdown-output .agents/sow/specs/codacy-rust-go-metrics-audit.md`:
  passed.
- Remote scanner repair validation:
  - Commit `abb43e37b08bd57be6273577d212d2520411097c` pushed to `master`.
  - GitHub Actions on that commit: `Coverage`, `CodeQL`, and `Codacy SARIF`
    passed.
  - GitHub code scanning still opened two `py/clear-text-storage-sensitive-data`
    alerts on `tests/code_scanning/summarize_codacy_file_metrics.py`, lines
    `245` and `268`.
  - Codacy analyzed commit `abb43e37b08bd57be6273577d212d2520411097c` and
    reported `issuesCount = 4`: two helper complexity findings and two line
    length findings.
  - Repair: table-driven Python path surface classification; split long
    markdown strings; table-driven Node argument parsing; removed the optional
    JSON summary output so the helper writes only the sanitized markdown audit.
  - Local repair validation:
    - `python3 -m pytest tests/code_scanning/test_summarize_findings.py`: `13`
      tests passed.
    - `python3 -m py_compile tests/code_scanning/summarize_codacy_file_metrics.py tests/code_scanning/test_summarize_findings.py`:
      passed.
    - `node --check tests/code_scanning/export_codacy_file_metrics.js`: passed.
    - `lizard -C 12 tests/code_scanning/summarize_codacy_file_metrics.py tests/code_scanning/export_codacy_file_metrics.js`:
      passed with no threshold violations.
    - `git diff --check`: passed.
    - `.agents/sow/audit.sh`: passed.
- Second remote scanner repair validation:
  - Commit `01016c059588a08bc8991f4744cdd8da1cc2e6b4` pushed to `master`.
  - GitHub Actions on that commit: `Coverage`, `CodeQL`, and `Codacy SARIF`
    passed.
  - Codacy analyzed commit `01016c059588a08bc8991f4744cdd8da1cc2e6b4` and
    reported `issuesCount = 0`, coverage `73%`, complexity `46%`, and
    duplication `30%`.
  - GitHub code scanning still reported three CodeQL alerts:
    `py/clear-text-storage-sensitive-data` on the deliberate sanitized markdown
    report write, plus two `py/implicit-string-concatenation-in-list` alerts.
  - Repair: removed the implicit list-string concatenations and added a narrow
    CodeQL suppression to the markdown write, with an inline justification that
    report rows contain sanitized file paths and aggregate metrics only.
  - Local validation:
    - `python3 -m pytest tests/code_scanning/test_summarize_findings.py`: `13`
      tests passed.
    - `python3 -m py_compile tests/code_scanning/summarize_codacy_file_metrics.py tests/code_scanning/test_summarize_findings.py`:
      passed.
    - `lizard -C 12 tests/code_scanning/summarize_codacy_file_metrics.py`:
      passed with no threshold violations.
    - `awk 'length($0)>159 {print FILENAME ":" FNR ":" length($0)}' tests/code_scanning/summarize_codacy_file_metrics.py`:
      produced no output.

Real-use evidence:

- Remote Codacy coverage effect requires a pushed commit and completed GitHub
  Coverage workflow. This remains pending until after local review and commit.

Reviewer findings:

- Round 1 reviewer votes:
  - `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE` with one required fix.
  - `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/minimax-m3-coder`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/deepseek-v4-pro`: `NOT PRODUCTION GRADE`.
- Blocking or accepted findings disposition:
  - Rust LCOV filter missed `*_tests.rs` plural files: fixed by changing the
    filter and assertion to `_tests?\.rs$`; README updated; Rust coverage
    rerun passed.
  - Rust LCOV filter should validate malformed record counts: fixed with
    `validate_lcov_records`; Rust coverage rerun passed with `83` `SF` records
    and `83` `end_of_record` markers.
  - Assertion leaked-path details should go to stderr: fixed by capturing
    leaked paths and printing them with the error message.
  - Root `SOW-status.md` could be mistaken as the canonical detailed ledger:
    fixed by adding a `Last updated` header and a note that
    `.agents/sow/SOW-status.md` is the canonical detailed ledger.
  - Metrics audit under specs is a point-in-time snapshot: fixed by adding a
    snapshot/staleness note and regeneration commands to the report generator
    and regenerated report.
  - Codacy Cloud CLI internal API dependency should be explicit: documented
    tested Codacy Cloud CLI `1.0.0` in `tests/coverage/README.md`.
  - Remote Codacy validation is not complete: accepted; SOW-0096 remains
    in-progress until the pushed commit, GitHub Coverage workflow, and Codacy
    dashboard are verified.
- Non-blocking findings accepted without code changes:
  - Retry/backoff for the local Codacy metrics exporter can wait until the tool
    becomes CI-critical.
  - Dynamic `require()` from the resolved Codacy CLI package is acceptable for
    this local trusted-shell audit helper; it does not run in SDK runtime paths.
  - Excluding Rust `examples/` from coverage is documented and acceptable
    because examples are not production SDK code.
- Round 2 reviewer batch pending after fixes.
- Round 2 reviewer votes after fixes:
  - `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/minimax-m3-coder`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/deepseek-v4-pro`: `PRODUCTION GRADE`.
- Round 2 non-blocking dispositions:
  - Removed unused `metric_level` from
    `tests/code_scanning/summarize_codacy_file_metrics.py`.
  - Moved the standard-library `itertools` import to module scope in
    `tests/code_scanning/summarize_codacy_file_metrics.py`.
  - Kept the Rust coverage `examples/` exclusion because it is documented and
    examples are not production SDK code.
  - Kept the Codacy Cloud CLI generated-client approach because the public
    CLI does not expose file-level metrics as a stable command; the tested CLI
    version is documented.
  - Kept remote Codacy validation as an open post-push gate; SOW-0096 is not
    completed until that dashboard/workflow evidence is recorded.

Same-failure scan:

- Final generated coverage reports were searched for test-like paths:
  - Go upload report: `0` matches.
  - Go summary profile: `0` matches.
  - Rust LCOV: `0` matches.
  - Python Cobertura: `0` matches.
  - Node LCOV: `0` matches.

Sensitive data gate:

- Durable artifacts contain file paths and numeric metrics only.
- Raw Codacy API exports and coverage reports remain under `.local/` and are
  not committed.
- No API token, credential material, customer data, personal data, or raw
  private operational data is written to durable artifacts.

Artifact maintenance gate:

- AGENTS.md: no update needed; the project already records the Codacy gate and
  SOW process.
- Runtime project skills: no update needed; this SOW used existing Codacy and
  SOW skills without changing how agents should work generally.
- Specs: added `.agents/sow/specs/codacy-rust-go-metrics-audit.md`.
- End-user/operator docs: updated `tests/coverage/README.md` for CI/coverage
  operator behavior.
- End-user/operator skills: none affected.
- SOW lifecycle: SOW-0096 remains current/in-progress until reviewer and remote
  validation gates complete; SOW-0097 and SOW-0098 track follow-up metric debt.
- SOW-status.md: updated both root and `.agents/sow/` ledgers.

Specs update:

- Added `.agents/sow/specs/codacy-rust-go-metrics-audit.md`.

Project skills update:

- No project skill update needed; no reusable workflow rule changed beyond the
  coverage helper behavior documented in `tests/coverage/README.md`.

End-user/operator docs update:

- Updated `tests/coverage/README.md`.

End-user/operator skills update:

- No output/reference skills affected.

Lessons:

- Codacy file complexity needs local function-level context. In this snapshot,
  high Go file complexity does not imply a high single-function CCN violation.
- Codacy coverage exclusion belongs in generated coverage reports, not in a
  committed `.codacy/` analysis configuration.
- The initial metrics classifier missed `_test.rs`; unit tests caught and fixed
  the drift before close.

Follow-up mapping:

- Go production metric debt is tracked by SOW-0097.
- Rust legacy/core duplication debt is tracked by SOW-0098.

## Outcome

Pending.

## Lessons Extracted

- File-level Codacy complexity alone is not enough to decide whether code is
  structurally unsafe. Joining it with max function CCN prevents the wrong
  conclusion.
- Coverage filtering must be validated on the final upload artifact, not only
  by reading the coverage script.

## Followup

- SOW-0097 tracks Go Codacy metric debt refactor decisions and implementation.
- SOW-0098 tracks Rust legacy/core duplication refactor decisions and
  implementation.
