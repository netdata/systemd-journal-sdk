# SOW-0119 - Codacy Severe Findings Except Go Version

## Status

Status: completed

Sub-state: regression repair complete. Codacy reanalysis of pushed commit
`05340b3e7baa8fe44736e0c4f0bc2bf919df36cd` reports 23 total issues and all
23 are the accepted `go/go.mod` SCA cluster. The Go language directive remains
unchanged by explicit user decision.

## Requirements

### Purpose

Keep Codacy useful as a high-signal gate without breaking current Netdata
consumers. The cleanup must improve real code quality and scanner signal while
preserving the existing Go module compatibility contract.

### User Request

The user asked to check severe Codacy findings. After reviewing the current
Codacy results, the user instructed:

- do not change the Go version because Netdata will fail to use the library if
  Netdata is not updated too;
- fix everything except the Go-version-related findings.

### Assistant Understanding

Facts:

- Codacy Cloud for `gh/netdata/systemd-journal-sdk` analyzed `master` commit
  `a3aa19d56b3e0e9690b54ba33893f5b86fb9214c` on 2026-06-17.
- Codacy reports 82 Critical/High-or-Error issues on that commit.
- Fourteen `Trivy_vulnerability_high` rows are all tied to `go/go.mod:3`
  advertising `go 1.26`, which Codacy maps to `golang/stdlib@v1.26.0`.
- The local workstation Go toolchain is `go1.26.4`, but the repository module
  directive remains `go 1.26`.
- The user explicitly rejected changing the Go version in this SOW.

Inferences:

- The Go stdlib SCA findings remain expected after this SOW unless Codacy can be
  configured to understand the patched toolchain without changing the module
  directive. This SOW will not attempt that because it would be a separate
  Codacy/toolchain-policy decision.
- Remaining severe findings are mostly scanner-actionable refactors, unused
  code cleanup, test-helper complexity reduction, retired experiment hygiene,
  and audited Rust FFI unsafe suppressions.

Unknowns:

- Whether Codacy Cloud will fully clear all non-Go-version findings before the
  branch is pushed and reanalyzed. Local validation will use same-pattern
  searches and language tests; remote closure requires Codacy reanalysis.

### Acceptance Criteria

- `go/go.mod` remains at `go 1.26`.
- All current Codacy Critical/High/Error issue patterns except the
  `go/go.mod` Go stdlib SCA cluster are fixed, locally suppressed with
  evidence, or explicitly dispositioned in this SOW.
- Rust `unsafe` findings in the optional host helper are audited and suppressed
  only where required FFI remains justified by the optional-helper contract.
- Product Rust and Go behavior is preserved by focused tests and full relevant
  language test suites.
- Retired Python/Node experiment findings are fixed without re-promoting those
  languages into product scope.
- Same-failure scans show no remaining local occurrences for the addressed
  severe patterns, excluding `go/go.mod`.
- SOW audit and whitespace checks pass.

## Analysis

Sources checked:

- `AGENTS.md`
- `.agents/skills/project-agent-orchestration/SKILL.md`
- `.agents/skills/project-journal-compatibility/SKILL.md`
- `/home/costa/.agents/skills/codacy-cloud-cli/SKILL.md`
- `/home/costa/.agents/skills/codacy-analysis-cli/SKILL.md`
- `.agents/sow/done/SOW-0084-20260602-code-scanning-and-codacy-gate.md`
- `.agents/sow/done/SOW-0096-20260607-codacy-metrics-and-coverage-hygiene.md`
- `.agents/sow/specs/product-scope.md`
- Codacy Cloud CLI issue and finding exports for `gh/netdata/systemd-journal-sdk`

Current state:

- Codacy repository details: 124 total issues, 33 High security findings, 72%
  coverage, 11% complex files, 30% duplication.
- Severe non-Go-version issue clusters:
  - Go/Rust Lizard complexity in production and test helpers.
  - Python pyflakes/pylint/bandit findings in test and retired experiment code.
  - JavaScript PMD numeric literal findings in retired Node experiment tests.
  - Rust Semgrep unsafe-usage findings in the optional host helper FFI layer.
- Product scope now targets Rust and Go only; Python and Node live under
  `experiments/` and remain retired by SOW-0116.

Risks:

- Refactoring complexity findings in journal verifier/writer paths can change
  file-format behavior if helper extraction is not mechanical.
- Suppressing Rust unsafe findings without audit would hide real FFI risks.
- Fixing retired experiment code can waste effort or accidentally imply renewed
  product support. This SOW treats experiment fixes as scanner hygiene only.
- Leaving Go stdlib SCA findings unresolved is intentional but means Codacy may
  still show High findings until Netdata can accept a Go directive bump or
  Codacy configuration changes.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Codacy severe findings are currently a mix of real static-analysis hygiene
  issues and one intentionally constrained compatibility problem. The Go SCA
  cluster is caused by repository metadata, not the local toolchain, and cannot
  be fixed in this SOW without violating the user's Netdata compatibility
  constraint. Other severe rows can be addressed by scoped refactors,
  cleanup, or documented suppressions.

Evidence reviewed:

- Codacy repository JSON for `master` commit
  `a3aa19d56b3e0e9690b54ba33893f5b86fb9214c`.
- Codacy issue export filtered by `--severities Critical,High --limit 1000`.
- Codacy findings export filtered by `--severities Critical,High --limit 1000`.
- `go/go.mod:3` shows `go 1.26`.
- Local `go version` reports `go1.26.4 linux/amd64`.
- `.agents/sow/specs/product-scope.md` states Rust and Go are the only product
  language targets and Python/Node are retired experiments.

Affected contracts and surfaces:

- Go journal writer, verifier, reader-access, and Netdata function helpers.
- Rust sealed verifier and optional `journal-host` helper FFI.
- Test and validation harnesses under `tests/`.
- Retired Python/Node experiment code under `experiments/`.
- Codacy severe finding status.
- SOW status summary.

Existing patterns to reuse:

- Mechanical helper extraction for Lizard reductions.
- Existing `SAFETY:` comments around Rust FFI calls.
- Existing Python test helper style and explicit return objects.
- Existing Node test constants and deterministic fixture timestamps.
- `.local/` for any raw scanner output if retained.

Risk and blast radius:

- Medium for product Go/Rust code because verifier and writer helpers are
  behavioral hot paths; tests must prove no compatibility regression.
- Low for retired experiments because they are outside product scope, but they
  still need syntax/runtime validation where practical.
- Low for audited Rust FFI suppressions if each unsafe block keeps a specific
  safety rationale and stays in optional helper code.
- Codacy dashboard will still report Go stdlib SCA findings by design.

Sensitive data handling plan:

- Do not write Codacy tokens, credentials, account details, personal data, raw
  scanner logs, or private URLs to durable artifacts.
- Durable artifacts may include repository paths, public commit hashes, pattern
  IDs, counts, and sanitized summaries.
- Raw Codacy JSON, if saved, stays under `.local/`.

Implementation plan:

1. Record this SOW and update `.agents/sow/SOW-status.md`.
2. Fix scanner-actionable Python and Node retired-experiment/test findings.
3. Refactor Go and Rust Lizard findings with mechanical helper extraction and
   no contract changes.
4. Audit and suppress required optional-helper Rust FFI unsafe blocks with
   `SAFETY:` evidence.
5. Run focused same-failure scans and relevant test suites.
6. Record validation and remaining Go-version SCA disposition.

Validation plan:

- `go test ./...` under `go/`.
- Relevant Rust tests or workspace checks for touched Rust packages.
- Python syntax/tests for touched harnesses.
- Node experiment test command where available, or at minimum syntax checks for
  touched files if the retired experiment suite has no maintained runner.
- Local Lizard checks for touched functions.
- Same-failure `rg` scans for the severe patterns fixed in this SOW.
- `git diff --check`.
- `.agents/sow/audit.sh`.

Artifact impact plan:

- AGENTS.md: no update expected; workflow contract unchanged.
- Runtime project skills: no update expected; no new durable workflow rule.
- Specs: no update expected unless implementation exposes a changed product
  contract.
- End-user/operator docs: no update expected; no user-facing behavior change.
- End-user/operator skills: no update expected.
- SOW lifecycle: new current SOW; close only after validation and follow-up
  mapping.
- SOW-status.md: update current work summary.

Open-source reference evidence:

- No external open-source implementation evidence is needed. This is scanner
  hygiene for existing local code, not a protocol or API design change.

Open decisions:

- Decision already provided by the user: keep the Go module directive unchanged
  and fix everything else.

## Implications And Decisions

1. Go module directive handling
   - Selected option: keep `go/go.mod` at `go 1.26`.
   - Reasoning: the user stated Netdata will fail to use the library if the SDK
     raises the Go version before Netdata is updated.
   - Implication: the Codacy Go stdlib SCA cluster may remain visible after this
     SOW.
   - Risk: Codacy will still show High findings that are not actionable in this
     repository until the consumer compatibility constraint changes or Codacy is
     configured differently.

## Plan

1. Establish the non-Go-version issue ledger and create this SOW.
2. Apply low-risk cleanup in Python/Node tests and retired experiments.
3. Refactor product Go/Rust complexity findings mechanically.
4. Audit required Rust optional-helper unsafe blocks and add precise scanner
   suppressions.
5. Validate locally, update SOW evidence, and leave the Go stdlib cluster as an
   explicit accepted exception.

## Delegation Plan

Implementer:

- Local implementation in this session. External assistants are not invoked
  because the user did not request running them for this cleanup.

Reviewers:

- User decision on 2026-06-18: after local fixes and tests, all Rust and Go
  changes in this SOW must be reviewed read-only by Claude, Codex, glm,
  minimax, kimi, mimo, deepseek, and qwen until they vote `PRODUCTION GRADE`.
- Reviewer scope is the Rust and Go changed surface in this SOW, plus tests
  needed to verify those changes. Python and Node cleanup is included in local
  validation but not in the required external review scope unless a reviewer
  identifies a cross-surface risk.

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

- If local validation exposes behavior changes, revert only the specific local
  edit by patch and preserve unrelated user work.
- If a severe finding cannot be fixed without a design or compatibility change,
  record the evidence and ask the user for a numbered decision.
- If SOW audit fails, repair the SOW framework issue before closure.

## Execution Log

### 2026-06-18

- Created SOW after Codacy Cloud triage and user decision to keep Go module
  version unchanged.
- Recorded user decision requiring Claude, Codex, glm, minimax, kimi, mimo,
  deepseek, and qwen read-only review of all Rust and Go changes until
  `PRODUCTION GRADE`.
- Fixed all locally actionable non-Go-version severe Codacy issue classes:
  Go/Rust Lizard complexity findings, Python Pyflakes/Pylint/Bandit severe
  findings, JavaScript PMD numeric literal findings, and Rust Semgrep unsafe
  findings.
- Preserved `go/go.mod` at `go 1.26`; the Go stdlib SCA cluster remains an
  explicit accepted exception.
- Ran local validation and external Rust/Go reviewer gate.

### 2026-06-19

- Reopened this SOW as a regression after Codacy Cloud reanalysis of pushed
  commit `e17f694254559ad9456335c898063e75be00fb13` reported 62 total issues.
- Classified the remaining rows as 23 accepted `go/go.mod` SCA findings and 39
  non-Go findings to fix or explicitly disposition.
- Pushed repair commit `0e243494aa6c8240b402bc38ea73b08cdc6f5924`; Codacy
  reanalysis reduced the count to 24 and exposed one remaining non-Go Lizard
  parameter-count row in `experiments/python/journal/explorer.py`.
- Folded the Python Explorer main-row early-stop flags into one tuple so
  `_handle_main_scanned_row` stays under the Codacy/Lizard 12-parameter limit.

## Validation

Acceptance criteria evidence:

- `go/go.mod:3` remains `go 1.26`; `git diff -- go/go.mod` is empty.
- Codacy severe export for analyzed commit
  `a3aa19d56b3e0e9690b54ba33893f5b86fb9214c` contained 82 Critical/High rows.
  Fourteen are the accepted Go stdlib SCA cluster tied to `go/go.mod:3`.
- Codacy-reported non-Go Lizard files pass `lizard -C 12` with no thresholds
  exceeded.
- Rust `journal-host` unsafe blocks now have local `SAFETY:` evidence and
  Semgrep suppressions only on required optional-helper FFI calls.
- Python and Node retired-experiment severe scanner findings were cleaned up
  without changing product scope.

Tests or equivalent validation:

- `go test ./...` from `go/` passed.
- `cargo test --workspace --all-targets` from `rust/` passed.
- `cargo fmt --all -- --check` passed.
- `gofmt -l` on touched Go files returned no files.
- `npm test` from `experiments/node/` passed after fixing stale repo-level
  fixture roots in the retired Node test harness.
- Repo-local Python venv validation passed:
  - `experiments/python/test_all.py`;
  - `python -m unittest discover -s experiments/python -p 'test*.py'`;
  - `python -m unittest discover -s tests -p 'test*.py'`.
- `python3 tests/docs/verify_examples.py --timeout 60` passed 31/31 verified
  Rust and Go examples.
- `python3 -m compileall -q tests experiments/python experiments/node/test`
  passed.
- `node --check` on touched Node files passed.
- Targeted Python severe-linter checks passed:
  - Pyflakes on touched Python files;
  - Pylint `E0601,W0101,W0106` on affected files;
  - Bandit `B105,B112` on affected files.
- `semgrep --config p/rust --error --quiet` on touched Rust `journal-host`
  files passed.
- `git diff --check` passed.
- `.agents/sow/audit.sh` passed.

Regression repair validation on 2026-06-19:

- `git diff -- go/go.mod` is empty; `go/go.mod:3` remains `go 1.26`.
- `lizard -C 12 experiments/python/journal/explorer.py
  experiments/python/test_all.py` passed with zero threshold warnings.
- After Codacy reported one remaining parameter-count row,
  `lizard -C 12 -a 12 experiments/python/journal/explorer.py
  experiments/python/test_all.py` passed with zero threshold warnings.
- `bandit -q -r tests/docs/verify_examples.py
  tests/docs/test_verify_examples.py` passed.
- `pylint --disable=all --enable=C0200,W0107
  tests/docs/test_verify_examples.py experiments/python/journal/explorer.py`
  passed.
- `python3 -m py_compile experiments/python/journal/explorer.py
  experiments/python/test_all.py tests/docs/verify_examples.py
  tests/docs/test_verify_examples.py` passed.
- `markdownlint-cli2` with only unrelated default `MD013` and `MD060`
  disabled passed on the active SOW, status files, touched SOW history files,
  docs-authoring skill, and touched docs pages.
- `python3 -m unittest tests.docs.test_verify_examples` passed, 50 tests.
- `python3 tests/docs/check_wiki_docs.py` passed, 15 wiki markdown files.
- `.local/python-test-venv/bin/python experiments/python/test_all.py` passed.
- `.local/python-test-venv/bin/python -m unittest discover -s
  experiments/python -p 'test*.py'` passed, 240 tests.
- After the parameter-count cleanup, `.local/python-test-venv/bin/python -m
  unittest discover -s experiments/python -p 'test*.py'` passed again, 240
  tests.
- `.local/python-test-venv/bin/python -m unittest discover -s tests -p
  'test*.py'` passed, 11 tests.
- `python3 tests/docs/verify_examples.py --timeout 60` passed, 31/31 verified
  Rust and Go examples, with Rust/Go caches redirected under `.local/`.
- `go test ./...` from `go/` passed with repo-local Go caches.
- `cargo test --workspace --all-targets` from `rust/` passed with repo-local
  Cargo caches.
- `git diff --check` passed.
- No Rust or Go source files were changed by this regression repair, so the
  user-required Rust/Go external reviewer gate from the original SOW is not
  rerun for the 2026-06-19 Python/docs-only repair.
- Codacy Cloud reanalysis of pushed commit
  `05340b3e7baa8fe44736e0c4f0bc2bf919df36cd` started on
  2026-06-19T03:33:06.909Z and ended on 2026-06-19T03:33:38.679Z with
  `issuesCount = 23`.
- Final Codacy issue export for that commit contains 23 rows, all in
  `go/go.mod`.
- Final Codacy Critical/High issue export for that commit contains 14 rows, all
  in `go/go.mod`.
- No non-Go Codacy issues remain in the final remote export.

Real-use evidence:

- Rust and Go docs examples built and ran against generated synthetic journal
  fixtures: 31/31 passed.
- Rust workspace and Go module tests exercised the product SDK surfaces touched
  by this SOW.
- Retired Python and Node aggregate package tests now run successfully after
  repairing stale fixture-root assumptions.

Reviewer findings:

- Required Rust/Go reviewer gate passed.
- Round 1 `PRODUCTION GRADE` verdicts:
  Claude, Codex, glm, minimax, kimi, mimo, and qwen.
- Deepseek round 1 timed out after running repository checks and did not emit a
  verdict; deepseek rerun, with the same Rust/Go review scope and no full-suite
  reruns, returned `VERDICT: PRODUCTION GRADE`.
- No reviewer reported a blocking Rust/Go bug, regression, security issue,
  runtime-purity violation, or compatibility risk.
- Multiple reviewers independently identified the `go/journal/writer.go`
  append-open boot-ID error path cleanup as a real resource-leak fix rather
  than a regression.
- Raw reviewer logs are under `.local/sow0119/reviews/` and are not durable
  artifacts.

Same-failure scan:

- Codacy-reported Lizard severe file list passes locally with no thresholds
  exceeded.
- Rust Semgrep unsafe-usage scan passes on touched host-helper files.
- Targeted Bandit severe rules `B105,B112` pass on affected Python files.
- Targeted Pyflakes and Pylint severe/error classes pass on affected Python
  files.
- Node syntax checks pass on touched files; PMD numeric-literal findings were
  addressed by named constants.
- Codacy Cloud will require a pushed branch and reanalysis to update remote
  issue state; the accepted Go SCA rows are expected to remain visible until
  the consumer compatibility constraint changes or Codacy configuration changes.

Sensitive data gate:

- No raw credentials, Codacy tokens, personal data, customer data, account
  identifiers, or private endpoints were written to durable artifacts.
- Raw Codacy export and reviewer logs were kept under `.local/sow0119/`.
- Durable artifacts contain only sanitized counts, public commit hashes, file
  paths, command names, and reviewer verdict summaries.

Artifact maintenance gate:

- AGENTS.md: no update needed; workflow and project-wide contracts unchanged.
- Runtime project skills: no update needed; no new reusable workflow rule was
  introduced.
- Specs: no update needed; product behavior and public contracts are unchanged.
- End-user/operator docs: no update needed; no user-facing API or docs behavior
  changed.
- End-user/operator skills: no update needed; no output/operator skill changed.
- SOW lifecycle: this SOW is completed and will move to `.agents/sow/done/`.
- SOW-status.md: updated for completion.

Specs update:

- No spec update needed; the product contract is unchanged. The only behavior
  change is cleanup of an append-open error path resource leak, which does not
  change public API semantics.

Project skills update:

- No project skill update needed; this SOW did not change how agents should
  work in the repository.

End-user/operator docs update:

- No end-user/operator docs update needed; public Rust and Go behavior remains
  unchanged.

End-user/operator skills update:

- No end-user/operator skills update needed.

Lessons:

- In zsh, `status` is a read-only shell parameter. External-review wrappers
  should use `rc` or another variable name when capturing exit status.
- Retired experiment aggregate runners can silently drift after path/layout
  moves. Running the aggregate scripts, not only focused chunks, catches stale
  fixture-root assumptions.

Follow-up mapping:

- The Go stdlib SCA cluster is not deferred work in this SOW; it is an explicit
  accepted exception from the user decision to keep `go/go.mod` at `go 1.26`.
- Remote Codacy reanalysis after push is required to prove dashboard closure
  for non-Go-version findings.

## Outcome

Completed again after regression repair. All non-Go Codacy issues from the
62-row post-push dashboard were fixed or dispositioned. Final Codacy reanalysis
reports only the user-approved Go-version/stdlib SCA cluster. All local tests
and static checks listed above passed. All required Rust/Go reviewers from the
original Rust/Go change set returned `PRODUCTION GRADE`; no Rust or Go source
files changed in the 2026-06-19 Python/docs-only regression repair.

## Lessons Extracted

- Use `rc`, not `status`, in zsh reviewer wrappers.
- Keep retired experiment aggregate tests runnable when touching retired
  experiment code for scanner hygiene.

## Followup

No new follow-up SOW is required. The remaining Go SCA rows are an accepted
compatibility exception until Netdata can consume a Go directive bump or Codacy
can be configured to model the patched toolchain without changing
`go/go.mod`.

## Regression Log

### Regression - 2026-06-19

What broke:

- Codacy Cloud reanalysis of pushed commit
  `e17f694254559ad9456335c898063e75be00fb13` ended on
  2026-06-19 with `issuesCount = 62`.
- The 62 rows are not all severe findings. The remaining total includes 23
  accepted `go/go.mod` Trivy SCA rows tied to the user-approved `go 1.26`
  directive and 39 non-Go findings.
- The severe non-accepted rows are Lizard complexity findings in retired Python
  experiment files. Lower-severity non-Go rows are Bandit/Pylint findings in
  docs-example tooling and markdownlint findings in docs/SOW artifacts.

Evidence:

- `.local/sow0119-regression/repository.json`: Codacy repository snapshot for
  analyzed commit `e17f694254559ad9456335c898063e75be00fb13`.
- `.local/sow0119-regression/all-issues.json`: 62 total issue rows.
- `.local/sow0119-regression/severe-issues.json`: 25 Critical/High issue rows,
  including 14 accepted `go/go.mod` SCA rows and 11 Python Lizard rows.
- `.local/sow0119-regression/high-findings.json`: 14 High security findings,
  all tied to `go/go.mod`.

Why previous validation missed it:

- The original closure focused on the initial severe export and local targeted
  checks. It did not wait for and parse the full post-push Codacy issue export
  before closing.
- The retired Python explorer Lizard rows were outside the original locally
  addressed severe file set and remained visible after reanalysis.
- The markdownlint and docs-tooling rows are lower severity than the original
  severe request, but the user now asked about the full 62-issue dashboard
  count, so they are in regression scope.

Repair plan:

- Keep `go/go.mod` unchanged at `go 1.26`.
- Fix or explicitly disposition every non-Go issue from the 62-row export.
- Validate locally with targeted Lizard, Bandit, Pylint, markdown/docs checks,
  relevant Python tests, `git diff --check`, and SOW audit.
- Push the repair, trigger Codacy reanalysis, and verify that no non-Go issues
  remain. The expected remaining dashboard count is the accepted Go SCA cluster
  unless a separate user decision allows Codacy-side suppression or a Go
  directive change.

Validation update:

- Final Codacy reanalysis evidence is recorded in `## Validation`.

Artifact updates needed:

- Update this SOW and both SOW status indexes.
- No spec or end-user docs behavior change is expected; docs-table formatting
  may be corrected only to satisfy markdownlint.
