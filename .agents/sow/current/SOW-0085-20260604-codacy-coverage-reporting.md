# SOW-0085 - Codacy Coverage Reporting

## Status

Status: in-progress

Sub-state: coverage ingested; repairing Codacy findings introduced by coverage
workflow

## Requirements

### Purpose

Make Codacy report coverage for this SDK so code quality, security, and coverage
remain visible before Netdata integration and later release work.

### User Request

Codacy currently does not report coverage. Configure the project so coverage is
generated and uploaded to Codacy.

The user provided the account-token coverage uploader environment shape:

- `CODACY_API_TOKEN`
- `CODACY_ORGANIZATION_PROVIDER=gh`
- `CODACY_USERNAME=netdata`
- `CODACY_PROJECT_NAME=systemd-journal-sdk`

### Assistant Understanding

Facts:

- Codacy Cloud reports `Coverage: N/A` for `netdata/systemd-journal-sdk`.
- The repository has CodeQL and Codacy SARIF workflows, but no coverage
  workflow.
- No committed coverage reports or coverage-generation configuration exists.
- The SDK has test runners for Rust, Go, Python, and Node.js.
- Codacy official coverage docs support both repository tokens and account
  tokens. The user selected the account-token environment shape.
- Codacy official advanced coverage docs require `--force-coverage-parser go`
  when uploading Go coverprofile output.
- Codacy's tagged coverage-reporter installer supports
  `CODACY_REPORTER_VERSION`. Codacy's artifact endpoint and GitHub latest
  release both resolved the current reporter version to `14.1.3` on
  2026-06-04.
- Codacy documentation says coverage report paths must match repository-root
  paths, and Codacy marks reports `Pending` when report paths do not match
  repository file paths.
- Codacy Coverage Reporter tag `14.1.3` source shows `--prefix` prepends the
  configured path before git-file matching, and the Go parser strips
  `github.com/org/repo/` from GitHub Go coverprofile paths.

Inferences:

- All four SDK languages should report coverage because the project has public
  SDK surfaces in all four languages.
- Coverage should upload reports without introducing a coverage-threshold gate
  until the first baseline is visible in Codacy.

Unknowns:

- The effective coverage percentage is unknown until the workflow runs on a
  pushed commit and Codacy ingests the reports.

### Acceptance Criteria

- A committed GitHub Actions workflow generates Rust, Go, Python, and Node.js
  coverage reports.
- The workflow uploads coverage to Codacy using `CODACY_API_TOKEN` plus the
  provided account-token identity variables.
- Go coverage upload uses Codacy's Go coverage parser.
- Tokenless runs do not leak secrets and do not write raw token data to durable
  artifacts.
- Local coverage scripts exist for repeatable developer validation.
- Local validation covers script syntax, at least one local coverage-generation
  path, workflow sanity, git diff hygiene, and SOW audit.

## Analysis

Sources checked:

- `.github/workflows/codeql.yml`
- `.github/workflows/codacy-sarif.yml`
- `node/package.json`
- `node/package-lock.json`
- `python/requirements.txt`
- `go/go.mod`
- `rust/Cargo.toml`
- Codacy docs: `https://docs.codacy.com/coverage-reporter/`
- Codacy docs: `https://docs.codacy.com/coverage-reporter/uploading-coverage-in-advanced-scenarios/`

Current state:

- CodeQL exists and scans Go, JavaScript/TypeScript, Python, and Rust.
- Codacy SARIF exists and uploads static-analysis SARIF to GitHub code scanning.
- Neither workflow generates nor uploads coverage.
- `node/package.json` has `npm test`, but no coverage dependency or script.
- `python/requirements.txt` only contains runtime compression dependency.
- Go can use native `go test -coverprofile`.
- Rust can use `cargo llvm-cov`; `cargo-llvm-cov` is present locally.

Risks:

- Rust coverage can be slower than normal tests because instrumentation rebuilds
  the workspace.
- Uploading partial coverage incorrectly can produce incomplete Codacy coverage.
- Missing CI secrets would leave Codacy coverage as `N/A`; the workflow must make
  that visible in the job summary.
- Token values are sensitive and must never be committed or logged.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Codacy reports coverage as `N/A` because no workflow generates supported
  coverage reports and no workflow runs Codacy Coverage Reporter.

Evidence reviewed:

- `.github/workflows/codeql.yml`: CodeQL only, no coverage commands.
- `.github/workflows/codacy-sarif.yml`: Codacy Analysis CLI SARIF only, no
  coverage commands.
- Local artifact search excluding `.git`, `.local`, `rust/target`, and
  `node/node_modules`: no coverage report files found.
- Codacy Cloud CLI repository output: `Coverage: N/A`.
- Codacy official docs state coverage setup requires generating reports and
  running Coverage Reporter.
- Codacy official docs state Go coverprofile upload requires
  `--force-coverage-parser go`.

Affected contracts and surfaces:

- GitHub Actions workflow behavior.
- Developer validation scripts under `tests/coverage/`.
- SOW status tracking.
- No runtime SDK API, file format, or compatibility behavior is changed.

Existing patterns to reuse:

- Workflow style from `.github/workflows/codacy-sarif.yml`: GitHub Actions,
  `.local/` artifacts, summaries, secret-based optional remote integration.
- Existing per-language test runners:
  - Rust workspace under `rust/`.
  - Go module under `go/`.
  - Python custom runner `python/test_all.py`.
  - Node custom runner `node/test/all.js`.

Risk and blast radius:

- CI runtime can increase. This affects CI cost and feedback time only.
- Coverage tooling dependencies run in CI and local scripts. They do not enter
  SDK runtime dependencies.
- Coverage upload depends on `CODACY_API_TOKEN` GitHub secret availability.

Sensitive data handling plan:

- Do not write token values to files, SOWs, logs, docs, or comments.
- Durable artifacts may mention only the secret name `CODACY_API_TOKEN` and
  non-sensitive repository identity variables.
- Coverage reports can contain source paths and line hit counts, but no journal
  payload data. Generated reports stay under `.local/` in local runs and CI
  artifacts.

Implementation plan:

1. Add coverage helper scripts under `tests/coverage/` for Rust, Go, Python,
   Node.js, and Codacy upload.
2. Add `.github/workflows/coverage.yml` with one coverage job per language and a
   final upload job.
3. Update SOW status tracking.
4. Validate scripts, run representative local coverage generation, run audit,
   then commit.

Validation plan:

- `bash -n tests/coverage/*.sh`
- Run local coverage generation for Go, Python, Node.js, and Rust when tooling is
  available.
- `git diff --check`
- `.agents/sow/audit.sh`
- After push, verify GitHub Actions coverage workflow and Codacy repository
  coverage status.

Artifact impact plan:

- AGENTS.md: no update expected; workflow policy already covers Codacy gates.
- Runtime project skills: no update expected; this is not journal runtime work.
- Specs: no product spec update expected; this is CI/reporting behavior.
- End-user/operator docs: add coverage README under `tests/coverage/`.
- End-user/operator skills: no update expected.
- SOW lifecycle: this SOW moves from current to done when verified.
- SOW-status.md: update for active and completed status.

Open-source reference evidence:

- Codacy docs were checked online because coverage uploader behavior is external
  service behavior and can change.

Open decisions:

- Resolved: use the user-provided account-token environment shape rather than a
  repository token.
- Resolved: generate coverage for all four SDK languages.
- Resolved: upload coverage but do not add a coverage percentage gate until the
  first baseline is known.

## Implications And Decisions

1. Coverage authentication

Selected: account-token upload using `CODACY_API_TOKEN`,
`CODACY_ORGANIZATION_PROVIDER=gh`, `CODACY_USERNAME=netdata`, and
`CODACY_PROJECT_NAME=systemd-journal-sdk`.

Implication: the GitHub repository needs a `CODACY_API_TOKEN` secret with
coverage-upload rights. Token values remain out of durable artifacts.

1. Coverage scope

Selected: Rust, Go, Python, and Node.js.

Implication: CI cost is higher than a single-language setup, but Codacy will not
misrepresent the multi-language SDK as partially unmeasured.

1. Coverage gate behavior

Selected: reporting-only until baseline is visible.

Implication: missing or low coverage will be visible, but a threshold decision
requires a later user decision after the first accurate baseline.

## Plan

1. Create local coverage scripts.
2. Add GitHub coverage workflow.
3. Validate locally.
4. Run whole-SOW reviewer pass if needed by the gate.
5. Commit and push after validation.

## Delegation Plan

Implementer:

- Local implementation by the project manager, matching current routing.

Reviewers:

- Reviewer pool after complete local validation if the change touches enough
  CI/security behavior to warrant it.

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

- If coverage generation fails for a language, record the failure and repair it
  before closing.
- If upload fails due missing secret, record it as an external configuration
  blocker and do not treat token absence as code correctness.

## Execution Log

### 2026-06-04

- Created SOW after confirming Codacy Cloud reports coverage as `N/A` and no
  coverage workflow exists.
- Added local coverage helpers under `tests/coverage/`.
- Added `.github/workflows/coverage.yml`.
- Fixed the shared shell `run()` helper to preserve failed command exit codes
  during validation.
- Switched Node.js coverage from `c8@latest` to
  `monocart-coverage-reports@2.12.12` after local Node 26 validation showed
  `c8@latest` fails before running the test suite with a CommonJS/ESM loader
  error.
- Filtered Node.js coverage to committed `node/src/**` sources so package
  manager caches, `node_modules`, tests, vendor payloads, adapters, and command
  fixtures do not pollute Codacy coverage.
- Suppressed Node.js coverage-tool deprecation warnings only for instrumented
  coverage runs because coverage tooling warnings on Node 26 polluted stderr
  for tests that intentionally assert empty stderr on journalctl subprocesses.
- Pinned the Codacy coverage reporter and tagged installer to `14.1.3`, and
  forced the reporter cache under `.local/codacy/coverage-reporter/`.
- Added workflow concurrency to avoid overlapping coverage uploads for the same
  branch/ref.
- Fixed coverage path mapping:
  - Go coverprofile paths are normalized from
    `github.com/netdata/systemd-journal-sdk/go/...` to `go/...` before upload.
  - Rust LCOV `SF:` paths are normalized from absolute workspace paths to
    `rust/...`.
  - Node.js and Python uploads use Codacy `--prefix node/` and
    `--prefix python/` because their generated reports are rooted inside their
    language directories.
- Rewrote durable docs to avoid token-assignment examples that trigger the SOW
  sensitive-data audit.
- Disabled shell xtrace in the Codacy upload script before token-bearing paths
  so accidental `bash -x` invocation does not echo upload commands in debug
  mode.
- Cleaned coverage script colored `printf` calls and validated the new scripts
  with ShellCheck to avoid introducing static-analysis noise into the clean
  Codacy gate.
- Pushed commit `8d3538c8df53` and observed GitHub Coverage workflow run
  `26940463211` fail during job setup because `actions/upload-artifact@v8` does
  not exist.
- Verified current GitHub Action release tags with `gh api` on 2026-06-04:
  `actions/upload-artifact` latest is `v7.0.1`,
  `actions/download-artifact` latest is `v8.0.1`, `actions/checkout` latest is
  `v6.0.3`, `actions/setup-go` latest is `v6.4.0`,
  `actions/setup-node` latest is `v6.4.0`, and `actions/setup-python` latest is
  `v6.2.0`.
- Repaired `.github/workflows/coverage.yml` to use
  `actions/upload-artifact@v7` while keeping `actions/download-artifact@v8`.
- Repaired the four Codacy findings reported after coverage ingestion:
  - `Semgrep_yaml.github-actions.security.third-party-action-not-pinned-to-commit-sha`
    reported `dtolnay/rust-toolchain@stable` and
    `taiki-e/install-action@cargo-llvm-cov` in
    `.github/workflows/coverage.yml`.
  - `markdownlint_MD029` reported ordered-list style at lines 219 and 226 in
    this SOW.
- Pinned third-party Rust coverage actions to full commit SHAs while preserving
  explicit inputs:
  - `dtolnay/rust-toolchain` stable ref:
    `29eef336d9b2848a0b548edc03f92a220660cdb8`
  - `taiki-e/install-action` cargo-llvm-cov ref:
    `28ba36d36bfc4814f98a469ff9f76b2a41e9aa8a`

## Validation

Acceptance criteria evidence:

- `.github/workflows/coverage.yml` runs separate Rust, Go, Python, and Node.js
  coverage jobs and uploads their artifacts.
- `.github/workflows/coverage.yml` upload job sets
  `CODACY_ORGANIZATION_PROVIDER=gh`, `CODACY_USERNAME=netdata`,
  `CODACY_PROJECT_NAME=systemd-journal-sdk`, and reads
  `CODACY_API_TOKEN` from GitHub secrets.
- `.github/workflows/coverage.yml` pins
  `CODACY_REPORTER_VERSION=14.1.3`.
- `tests/coverage/upload_codacy_coverage.sh` uploads Go with
  `--force-coverage-parser go` and uploads Rust, Node.js, and Python as partial
  reports before finalizing.
- `tests/coverage/upload_codacy_coverage.sh` downloads Codacy's tagged
  installer from `codacy/codacy-coverage-reporter` tag `14.1.3` and keeps
  reporter cache files under `.local/`.
- `tests/coverage/upload_codacy_coverage.sh` passes `--prefix node/` for
  Node.js LCOV and `--prefix python/` for Python Cobertura reports.
- `tests/coverage/run_go_coverage.sh` normalizes Go coverprofile paths to
  `go/...`; validation found 0 remaining
  `github.com/netdata/systemd-journal-sdk/go/...` path prefixes and 16,226
  `go/...` coverage records.
- `tests/coverage/run_rust_coverage.sh` normalizes Rust LCOV `SF:` paths to
  `rust/...`; validation found 0 absolute Rust `SF:` paths and 0 non-`rust/`
  Rust `SF:` paths.
- Local generated reports:
  - `.local/coverage/go/coverage.out`: 746,481 bytes after root-relative
    normalization; raw Go coverprofile: 1,379,295 bytes.
  - `.local/coverage/rust/lcov.info`: 1,488,553 bytes after root-relative
    normalization.
  - `.local/coverage/node/lcov.info`: 115,207 bytes; 23 `SF:` entries, all
    under `src/`.
  - `.local/coverage/python/cobertura.xml`: 251,948 bytes.

Tests or equivalent validation:

- `bash -n tests/coverage/lib.sh tests/coverage/run_go_coverage.sh tests/coverage/run_node_coverage.sh tests/coverage/run_python_coverage.sh tests/coverage/run_rust_coverage.sh tests/coverage/upload_codacy_coverage.sh`: passed.
- `shellcheck -x tests/coverage/lib.sh tests/coverage/run_go_coverage.sh tests/coverage/run_python_coverage.sh tests/coverage/run_node_coverage.sh tests/coverage/run_rust_coverage.sh tests/coverage/upload_codacy_coverage.sh`: passed.
- `actionlint .github/workflows/coverage.yml`: passed.
- `tests/coverage/run_go_coverage.sh`: passed; generated normalized Go
  coverprofile.
- `PATH="$PWD/.local/coverage-python-venv/bin:$PATH" tests/coverage/run_python_coverage.sh`: passed after installing `coverage 7.14.1` and `lz4 4.4.5` into `.local/coverage-python-venv`.
- `tests/coverage/run_node_coverage.sh`: passed with
  `monocart-coverage-reports 2.12.12`; generated LCOV.
- `tests/coverage/run_rust_coverage.sh`: passed with `cargo-llvm-cov 0.8.5`;
  generated root-relative normalized LCOV.
- `tests/coverage/upload_codacy_coverage.sh`: passed tokenless skip path with
  no token value present.
- `env -u CODACY_API_TOKEN tests/coverage/upload_codacy_coverage.sh /does/not/exist`: passed tokenless skip path after upload-script xtrace and logging cleanup.
- Shared shell `run()` helper failure path: passed by returning the failed
  command's status and printing the full error banner.
- Node.js LCOV source filter check: passed with 23 `SF:` entries and 0 entries
  outside `src/`.
- Go coverprofile path-normalization check: passed with first paths under
  `go/...`, 0 module-prefixed entries, and 16,226 `go/...` records.
- Rust LCOV path-normalization check: passed with first paths under
  `rust/src/...`, 0 absolute `SF:` entries, and 0 non-`rust/` `SF:` entries.
- `git diff --check`: passed.
- `.agents/sow/audit.sh`: passed.

Real-use evidence:

- GitHub Coverage workflow run `26940463211` failed before any tests ran because
  `actions/upload-artifact@v8` could not be resolved. This is a workflow action
  tag availability failure, not a language coverage-generation failure.
- GitHub Coverage workflow run `26940610266` passed on commit `3a2a5e44`:
  Go coverage completed in 48s, Python in 1m31s, Rust in 2m31s, Node.js in
  4m33s, and the Codacy upload job completed in 17s.
- Codacy Coverage Reporter logs for run `26940610266` showed each partial
  report upload succeeded and the final coverage notification was received
  successfully.
- Codacy Cloud repository summary after run `26940610266` reported
  `Coverage: 62.0%` for `netdata/systemd-journal-sdk`.
- Codacy Cloud also reported 4 quality/security issues introduced by this SOW;
  these are being repaired before closure.

Reviewer findings:

- First whole-SOW reviewer pass:
  - `llm-netdata-cloud/kimi-k2.6`: `NOT PRODUCTION GRADE`; found the shared
    shell `run()` helper did not preserve failure diagnostics under `set -e`,
    `monocart-coverage-reports@latest` was unpinned, and Codacy installer/report
    versioning was too floating.
  - `llm-netdata-cloud/qwen3.6-plus`: `NOT PRODUCTION GRADE`; found the same
    `run()` helper failure-mode issue, unpinned Node coverage dependency,
    unpinned Codacy reporter path, and missing scheduled coverage.
  - `minimax-coding-plan/MiniMax-M3`: `NOT PRODUCTION GRADE`; found Node LCOV
    included `.local/npm-cache`, `node_modules`, tests, vendor data, adapters,
    and command fixtures; also found missing `CODACY_REPORTER_VERSION` pin and
    missing workflow concurrency.
  - `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE` with low-severity
    hygiene findings covering the same `run()` helper, Node version pinning,
    broad deprecation suppression, repeated reporter downloads, and stale SOW
    status date.
  - `llm-netdata-cloud/glm-5.1`: first-round result was lost before final
    output capture and will be rerun in the second whole-SOW pass.
- Dispositions after first reviewer pass:
  - Fixed `run()` helper failure handling and validated the failing-command
    path.
  - Pinned Node coverage tooling to `monocart-coverage-reports@2.12.12`.
  - Pinned Codacy reporter and tagged installer to `14.1.3`.
  - Moved Codacy reporter downloads/cache under `.local/`.
  - Added weekly scheduled coverage.
  - Added workflow concurrency.
  - Filtered Node LCOV to committed `node/src/**` sources and validated 0 bad
    `SF:` entries.
  - Kept `--no-deprecation` scoped to coverage runs only; this prevents
    coverage-tool warnings from breaking stderr-sensitive CLI tests and does not
    affect normal package tests.
- Intermediate second reviewer run:
  - `llm-netdata-cloud/qwen3.6-plus`: `NOT PRODUCTION GRADE`; found missing
    root-relative path mapping for language-subdirectory coverage reports.
  - Other captured second-run reviewers reported `PRODUCTION GRADE` or
    non-blocking observations, but the run was superseded by the qwen finding
    and subsequent path-mapping fixes.
- Dispositions after intermediate second reviewer run:
  - Verified Codacy Coverage Reporter `14.1.3` source: `--prefix` prepends
    paths before git-file matching; Go parser strips GitHub module prefix to
    repository-relative Go paths.
  - Added Node.js and Python upload prefixes.
  - Normalized Rust LCOV to repository-relative `rust/...` paths.
  - Normalized Go coverprofile output to repository-relative `go/...` paths
    before upload, avoiding reliance on reporter-internal GitHub module-prefix
    stripping.
  - Bumped artifact download to current major `v8`; later post-push validation
    showed artifact upload's current major is `v7`, so upload was repaired to
    `actions/upload-artifact@v7`.
  - Disabled xtrace inside the upload script before token-bearing paths.
  - Re-ran syntax, workflow, Go coverage, root-relative path checks, tokenless
    upload skip, diff hygiene, and SOW audit.
- Final whole-SOW reviewer run after ShellCheck cleanup:
  - `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`.
  - `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`.
  - `minimax-coding-plan/MiniMax-M3`: `PRODUCTION GRADE`.
  - No blocking findings remained. Non-blocking observations covered expected
    post-push Codacy ingestion verification, standard Codacy installer
    supply-chain exposure from the pinned `get.sh` download, Node.js
    coverage-run-only deprecation suppression, and CI runtime/caching cost.
  - One reviewer command attempted package metadata inspection despite the
    read-only/no-package-manager prompt. It did not change repository files and
    did not affect the accepted technical findings.

Same-failure scan:

- Confirmed no pre-existing coverage workflow or upload path existed by scanning
  `.github/workflows/` for coverage, Codacy Coverage Reporter, `get.sh`,
  `coverprofile`, `llvm-cov`, `lcov`, `c8`, and `coverage.py` terms.

Sensitive data gate:

- Durable artifacts mention only secret names and non-sensitive repository
  identity variables. No raw token value was written.

Artifact maintenance gate:

- AGENTS.md: no update needed; existing Codacy/code-scanning gate policy remains
  accurate.
- Runtime project skills: no update needed; this work does not change project
  execution rules.
- Specs: no update needed; this work does not change SDK product behavior.
- End-user/operator docs: added `tests/coverage/README.md`.
- End-user/operator skills: no update needed; no output/reference skill is
  affected.
- SOW lifecycle: SOW remains `in-progress` in `current/` until reviewer and
  pushed-workflow validation complete.
- SOW-status.md: updated with active SOW-0085 status.

Specs update:

- No product spec update needed; coverage reporting is CI/reporting
  infrastructure, not SDK behavior.

Project skills update:

- No project skill update needed; current SOW and Codacy rules covered the work.

End-user/operator docs update:

- Added `tests/coverage/README.md`.

End-user/operator skills update:

- No output/reference skills are affected.

Lessons:

- Node.js coverage tooling can interact with package-level ESM and CLI stderr
  assertions. The working coverage path uses V8-native
  `monocart-coverage-reports` and suppresses only coverage-tool deprecation
  warnings for instrumented runs.

Follow-up mapping:

- After the repaired workflow is pushed, verify the GitHub Coverage workflow and
  Codacy coverage ingestion.
- A coverage percentage threshold is intentionally not set until the first
  accurate baseline is visible in Codacy.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

Pending.
