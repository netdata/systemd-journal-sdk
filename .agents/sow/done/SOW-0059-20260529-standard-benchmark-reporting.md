# SOW-0059 - Standard Benchmark Reporting

## Status

Status: completed

Sub-state: completed. The generated benchmark report format is implemented,
validated, reviewed, documented, and ready for use by subsequent performance
SOWs.

## Requirements

### Purpose

Make benchmark reporting fit for performance decisions by producing consistent,
generated tables with stable row ordering, stable columns, and explicit
comparison baselines.

### User Request

The user said the benchmark numbers were being presented in different shapes,
with systemd, Rust, and Go appearing inconsistently, and asked to standardize
the reporting.

### Assistant Understanding

Facts:

- Reader benchmark `summary.json` already records stable dimensions:
  `language`, `surface`, `mode`, `bounds`, `mmap_strategy`, and median/min/max
  rows per second.
- Writer-core benchmark output records language summaries and systemd ratios in
  `report.json`.
- The prior manual reports mixed comparison frames, making results hard to
  compare across turns.

Inferences:

- A generated Markdown report should become the only accepted shape for new
  benchmark summaries.
- Reader production comparisons and optimization before/after comparisons need
  different sections, but the section order and columns must remain fixed.

Unknowns:

- Whether new benchmark harnesses add new surfaces. The reporter should
  include unknown rows after known production/diagnostic rows rather than hide
  them.

### Acceptance Criteria

- Add a repository-local benchmark report generator under `tests/benchmarks/`.
- The generator reads benchmark result directories or JSON result files and
  emits Markdown with fixed section order.
- Reader reports always separate production comparison, diagnostic modes, and
  optional before/after change comparison.
- Writer-core reports always order languages as systemd, Rust, Go, Node.js,
  Python and include ratios versus systemd where available.
- Benchmark README documents the standard report shape and example commands.
- Generated sample output is validated against existing SOW-0058 benchmark
  artifacts.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `tests/benchmarks/run_reader_core_benchmarks.py`
- `tests/benchmarks/run_writer_core_benchmarks.py`
- `tests/benchmarks/README.md`
- `.agents/sow/specs/product-scope.md`
- `.agents/sow/done/SOW-0058-20260529-rust-data-header-fast-path.md`

Current state:

- Reader-core results are a list in `summary.json`.
- Reader-core `manifest.json` records run identity and configuration.
- Writer-core results are in `report.json`, with a `summary` object by
  language and a `parameters` object.
- There is no canonical report rendering tool, so benchmark summaries are
  manually assembled.

Risks:

- A report generator can create false confidence if it silently drops rows or
  guesses missing baselines.
- If benchmark rows are not ordered consistently, the tool would fail the main
  purpose of this SOW.
- If the report generator depends on non-standard packages, it adds friction to
  benchmark use.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The underlying benchmark data is structured, but presentation has been
  manual. Manual reshaping caused inconsistent comparison frames and order.

Evidence reviewed:

- `run_reader_core_benchmarks.py` writes `summary.json` plus `manifest.json`.
- `run_writer_core_benchmarks.py` writes `report.json`.
- SOW-0058 benchmark results were real but presented in multiple structures.

Affected contracts and surfaces:

- Benchmark reporting workflow.
- `tests/benchmarks/README.md`.
- SOW validation evidence for performance work.

Existing patterns to reuse:

- Existing benchmark output directories under `.local/benchmarks/`.
- Existing JSON field names from reader-core and writer-core harnesses.
- Existing no-extra-dependency Python benchmark scripts.

Risk and blast radius:

- Low. This adds reporting tooling and docs only; it does not change readers,
  writers, or benchmark measurement logic.

Sensitive data handling plan:

- The reporter reads generated benchmark JSON only.
- Durable artifacts record paths, numeric summaries, and configuration
  names only. Do not record real logs, SNMP communities, credentials, bearer
  tokens, customer data, personal data, private endpoints, or production
  incident details.

Implementation plan:

1. Add `tests/benchmarks/report_benchmarks.py` with reader-core and writer-core
   report support.
2. Add fixed reader production, diagnostic, and before/after table rendering.
3. Add fixed writer-core table rendering.
4. Document the standard report shape and commands in
   `tests/benchmarks/README.md`.
5. Validate using existing SOW-0058 reader artifacts.

Validation plan:

- Run report generator on a single reader-core result.
- Run report generator on reader before/after SOW-0058 artifacts.
- Run unit-style `compileall` syntax validation for the new script.
- Run `git diff --check`.
- Run `.agents/sow/audit.sh`.
- Whole-SOW read-only external review.

Artifact impact plan:

- AGENTS.md: no update expected; project-wide workflow does not change.
- Runtime project skills: no update expected unless reviewers identify this as
  a durable agent workflow rule.
- Specs: no product behavior change expected.
- End-user/operator docs: update `tests/benchmarks/README.md`.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: close after validation and review.
- SOW-status.md: update when this SOW opens and closes.

Open-source reference evidence:

- No external open-source checkout was needed. This is repository-local
  benchmark reporting tooling.

Open decisions:

- None. The user approved standardizing the benchmark reporting.

## Implications And Decisions

1. 2026-05-29 benchmark report format
   - Decision: new benchmark summaries should be generated from JSON using a
     stable report shape instead of hand-written comparison tables.
   - Implication: benchmark reports become reproducible and easier to compare
     across SOWs.

## Plan

1. Implement the report generator.
2. Document the canonical report sections and commands.
3. Validate against SOW-0058 benchmark artifacts.
4. Run read-only reviewers and close the SOW if clean.

## Delegation Plan

Implementer:

- Local implementation by the project manager, per current user routing.

Reviewers:

- Whole-SOW read-only review after local validation.

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

- If the generator cannot support a benchmark kind safely, it should fail with
  a clear error instead of guessing.

## Execution Log

### 2026-05-29

- Created SOW after user approved standardizing benchmark reports.
- Added `tests/benchmarks/report_benchmarks.py` as a stdlib-only Markdown
  renderer for reader-core `summary.json`/`manifest.json` and writer-core
  `report.json` artifacts.
- Updated `tests/benchmarks/README.md` with the canonical report sections,
  reader production row order, writer language order, conclusion labels, and
  example commands.
- Corrected writer before/after change output to use an `api` column rather
  than reusing the reader `mode` column.
- Addressed first reviewer batch:
  - writer-core run identity now uses `environment.timestamp_utc` instead of a
    profile directory name when available;
  - `latest` artifact resolution detects cycles and fails clearly;
  - reader change reports reject duplicate row keys instead of silently
    overwriting rows;
  - reader and writer change reports include an `Unmatched Rows` subsection
    for before-only and after-only rows;
  - writer API/access fields use defensive scalar/list formatting;
  - `--run` is rejected when combined with `--before` or `--after`.
- Addressed second reviewer batch:
  - production tables keep expected missing rows visible for configured
    languages with `status=missing`;
  - required benchmark metrics fail with clear `SystemExit` messages that
    include the artifact path, field name, and row context;
  - writer change reports surface same-language API/access differences in a
    `Configuration Differences` subsection;
  - added `tests/benchmarks/test_report_benchmarks.py` for missing production
    rows, malformed writer artifacts, unmatched rows, and writer configuration
    differences.
- Addressed final local validation findings:
  - invalid programmatic conclusion labels now fail with a clear `SystemExit`;
  - empty reader production sections render `_No matching rows._` instead of a
    bare section header;
  - `None` writer `parameters` and `environment` metadata are treated as empty
    objects instead of triggering raw tracebacks;
  - `open-files rust:core-payloads` is treated as diagnostic, while
    `file rust:core-payloads` remains a production row;
  - writer change-report configuration differences and writer before/after
    examples are documented in the benchmark README;
  - reader change reports now use the same production/diagnostic classification
    as single-run reports, so `open-files rust:core-payloads` stays
    diagnostic in both views;
  - diagnostic metric validation now reports the concrete
    `surface language mode` context;
  - reader change/unmatched access labels now reuse the same access-label helper
    as single-run reports;
  - unit coverage now includes duplicate reader keys, invalid conclusions,
    zero/none-reference ratio rendering, empty reader production sections, null
    metadata, reader unmatched rows, `latest` symlink cycles, writer missing
    configured-language rows, writer diagnostic-section omission, CLI conflict
    rejection, and `open-files rust:core-payloads` diagnostics in single-run
    and change reports.

## Validation

Acceptance criteria evidence:

- Report generator added:
  `tests/benchmarks/report_benchmarks.py`.
- Reader report support validated against SOW-0058 artifacts:
  `.local/benchmarks/reader-core-rust-data-header-fast-path-after-baseline/latest`.
- Reader before/after report support validated against SOW-0058 baseline and
  after artifacts:
  `.local/bench-ab-0058/base/.local/benchmarks/reader-core-rust-data-header-baseline/latest`
  and
  `.local/benchmarks/reader-core-rust-data-header-fast-path-after-baseline/latest`.
- Writer-core report support validated against existing writer-core artifacts:
  `.local/benchmarks/writer-core/compact-none-fss-off-api-raw-payload-live-every-1-mmap-windowed-20260528T201057735805Z/report.json`.
- Writer-core before/after report support validated against existing
  writer-core artifacts:
  `.local/benchmarks/writer-core/compact-none-fss-off-api-raw-payload-live-every-1-mmap-windowed-20260528T185420493448Z/report.json`
  and
  `.local/benchmarks/writer-core/compact-none-fss-off-api-raw-payload-live-every-1-mmap-windowed-20260528T201057735805Z/report.json`.
- Documentation added:
  `tests/benchmarks/README.md`.

Tests or equivalent validation:

- `python3 -m compileall -q tests/benchmarks/report_benchmarks.py`
  passed.
- `python3 tests/benchmarks/report_benchmarks.py --run .local/benchmarks/reader-core-rust-data-header-fast-path-after-baseline/latest --title 'SOW-0058 Reader Report' --conclusion mixed --conclusion-note 'Single-file SDK payload improved; open-files did not.'`
  produced fixed `Run Identity`, `Configuration`, `Production Comparison`,
  `Diagnostic Modes`, `Conclusion`, and `Raw Evidence` sections.
- `python3 tests/benchmarks/report_benchmarks.py --before .local/bench-ab-0058/base/.local/benchmarks/reader-core-rust-data-header-baseline/latest --after .local/benchmarks/reader-core-rust-data-header-fast-path-after-baseline/latest --title 'SOW-0058 Reader Change Report' --conclusion mixed --conclusion-note 'DATA header parsing is safe but benchmark signal is mixed.'`
  produced fixed production and diagnostic before/after change sections.
- `python3 tests/benchmarks/report_benchmarks.py --run .local/benchmarks/writer-core/compact-none-fss-off-api-raw-payload-live-every-1-mmap-windowed-20260528T201057735805Z/report.json --title 'Writer Core Benchmark' --conclusion not-assessed`
  produced systemd, Rust, Go, Node.js, Python rows in fixed language order.
- `python3 tests/benchmarks/report_benchmarks.py --before .local/benchmarks/writer-core/compact-none-fss-off-api-raw-payload-live-every-1-mmap-windowed-20260528T185420493448Z/report.json --after .local/benchmarks/writer-core/compact-none-fss-off-api-raw-payload-live-every-1-mmap-windowed-20260528T201057735805Z/report.json --title 'Writer Core Change Benchmark' --conclusion not-assessed`
  produced a writer change table with `api` and `access` columns.
- `python3 tests/benchmarks/report_benchmarks.py --help`
  printed CLI usage and accepted conclusion labels.
- `python3 tests/benchmarks/report_benchmarks.py --run .local/benchmarks/reader-core-rust-data-header-fast-path-after-baseline/latest --before .local/bench-ab-0058/base/.local/benchmarks/reader-core-rust-data-header-baseline/latest --after .local/benchmarks/reader-core-rust-data-header-fast-path-after-baseline/latest`
  failed intentionally with `--run cannot be combined with --before/--after`.
- A repository-local scratch symlink cycle under
  `.local/benchmarks/report-generator-cycle-smoke/latest` failed intentionally
  with `benchmark artifact path cycle detected ...`; the scratch directory and
  output file were removed afterward.
- A repository-local scratch reader artifact with configured Rust production
  rows missing `sdk-payloads` and `facade-data` produced visible
  `status=missing` rows; the scratch directory was removed afterward.
- A repository-local scratch malformed writer artifact missing
  `append_rows_per_second_median` failed intentionally with
  `missing append_rows_per_second_median ...`; the scratch directory and output
  file were removed afterward.
- `python3 tests/benchmarks/test_report_benchmarks.py`
  passed 15 stdlib unit tests.
- `python3 -m compileall -q tests/benchmarks/report_benchmarks.py tests/benchmarks/test_report_benchmarks.py`
  passed.
- `git diff --check -- tests/benchmarks/report_benchmarks.py tests/benchmarks/test_report_benchmarks.py tests/benchmarks/README.md .agents/sow/done/SOW-0059-20260529-standard-benchmark-reporting.md SOW-status.md .agents/sow/SOW-status.md`
  passed.
- `.agents/sow/audit.sh`
  passed after moving SOW-0059 to `done/`.

Real-use evidence:

- The reporter consumed real benchmark artifacts generated by prior SOWs under
  `.local/benchmarks/` and `.local/bench-ab-0058/`.
- The generated reader change report includes the SOW-0058 production deltas in
  a stable table, including `file rust sdk-payloads live/windowed` at
  `2,251,233 -> 2,511,708 rows/s` and `open-files rust sdk-payloads
  live/windowed` at `2,094,501 -> 2,011,444 rows/s`.
- The generated writer-core report includes fixed language ordering for
  `systemd`, `rust`, `go`, `node`, and `python`.

Reviewer findings:

- First reviewer batch:
  - `llm-netdata-cloud/minimax-m2.7-coder`: production-grade; noted
    `report_benchmarks.py` was untracked before commit staging.
  - `llm-netdata-cloud/kimi-k2.6`: production-grade with one recommended fix
    before close. Findings: writer-core `created_at` should use
    `environment.timestamp_utc`; `latest` recursion should have cycle
    protection; writer change reports should expose dropped systemd rows;
    terminal SOW gates need cleanup before close.
  - `llm-netdata-cloud/qwen3.6-plus`: produced validation/audit output but the
    final verdict was not fully captured in the terminal output budget. No
    blocking finding was captured.
  - `llm-netdata-cloud/glm-5.1`: production-grade; non-blocking findings:
    asymmetric before/after rows were silently omitted; duplicate reader keys
    could silently overwrite; `latest` recursion lacked cycle protection;
    `--run` plus `--before`/`--after` behavior was undocumented; scalar
    `api_modes` values would be formatted character-by-character.
- Disposition:
  - Fixed writer-core timestamp identity.
  - Fixed artifact path cycle handling.
  - Fixed duplicate reader key handling with an explicit failure.
  - Fixed asymmetric before/after row visibility with `Unmatched Rows`
    sections.
  - Fixed scalar/list writer field formatting.
  - Rejected combining `--run` with `--before`/`--after` and documented the
    intended modes.
  - Terminal SOW gates were cleaned before close.
- Second reviewer batch:
  - `llm-netdata-cloud/minimax-m2.7-coder`: production-grade; no blocking
    findings.
  - `llm-netdata-cloud/qwen3.6-plus`: production-grade; no blocking findings.
  - `llm-netdata-cloud/glm-5.1`: production-grade; suggested surfacing writer
    API/access differences, which was implemented while the review was still
    running.
  - `llm-netdata-cloud/kimi-k2.6`: not production-grade yet. Blocking findings:
    missing production rows were silently omitted, and malformed artifacts could
    produce raw `KeyError`/`TypeError`. Non-blocking findings: add automated
    tests; surface writer API/access differences; handle zero references
    explicitly; fill terminal SOW sections before close.
- Disposition:
  - Fixed missing production rows with visible `status=missing` rows scoped to
    configured languages.
  - Fixed required numeric fields with clear `SystemExit` errors.
  - Added stdlib unit tests for the new behavior.
  - Added writer `Configuration Differences` output.
  - Fixed ratio formatting for zero references with explicit `n/a`/`inf`.
  - Terminal SOW sections were filled before close.
- Final issue pass:
  - `llm-netdata-cloud/glm-5.1` probing exposed that
    `open-files rust:core-payloads` was excluded from production but still
    omitted from diagnostics because production filtering used unscoped
    production keys.
  - Disposition: fixed diagnostic filtering to use only expected production
    keys for the concrete surface and added
    `test_open_files_core_payloads_is_diagnostic`.
  - `llm-netdata-cloud/kimi-k2.6` identified null writer metadata as a
    blocker in malformed artifacts.
  - Disposition: fixed metadata type guards in identity/configuration rendering
    and added `test_null_metadata_sections_do_not_traceback`.
  - The same Kimi review flagged stale SOW validation text and README gaps for
    writer `Configuration Differences`, `open-files rust:core-payloads`, and
    writer before/after examples.
  - Disposition: updated validation evidence, documented the surface-specific
    `core-payloads` rule, documented writer configuration-difference output,
    added a writer change-report example, and added zero-reference ratio test
    coverage.
- Whole-SOW reviewer batch after local completion:
  - `llm-netdata-cloud/minimax-m2.7-coder`: production-grade; no blocking
    findings. Non-blocking findings: hardcoded configuration keys and missing
    CLI conflict unit coverage.
  - `llm-netdata-cloud/kimi-k2.6`: production-ready with minor fixes
    recommended. Findings: duplicate access-label logic, diagnostic errors
    lacked row identity, missing CLI conflict and writer missing-language unit
    coverage, and writer-core reports omit the diagnostic section so README
    section numbering can be confusing.
  - `llm-netdata-cloud/qwen3.6-plus`: not production-grade. Blocking finding:
    reader change reports misclassified `open-files rust:core-payloads` as
    production even though single-run reports classify it as diagnostic.
    Medium finding: `fmt_ratio_to(None, value)` crashed despite related helpers
    accepting `None`.
  - `llm-netdata-cloud/glm-5.1`: production-grade with minor findings:
    `fmt_ratio_to(None, value)` defensiveness gap and CLI/error-path test
    coverage gaps.
- Disposition:
  - Fixed reader change production/diagnostic classification by using one
    `is_reader_production_key()` predicate shared by change rendering and
    sorting.
  - Added `test_open_files_core_payloads_is_diagnostic_in_change_report`.
  - Made `fmt_ratio_to()` accept `None` values defensively and extended ratio
    tests.
  - Added CLI conflict and writer missing configured-language tests.
  - Reused the access-label helper in reader change/unmatched rows.
  - Improved diagnostic metric error contexts with row identity.
  - Documented writer-core section numbering behavior.
- Final Qwen/GLM re-review:
  - `llm-netdata-cloud/qwen3.6-plus`: production-grade. Non-blocking findings:
    one remaining inline access-label computation, missing explicit
    `fmt_ratio_to(0, nonzero)` coverage, and no test that writer-core reports
    omit `Diagnostic Modes`.
  - `llm-netdata-cloud/glm-5.1`: production-grade. Non-blocking finding:
    inline access-label computation for missing reader rows.
- Disposition:
  - Replaced the final inline access-label expression with
    `access_from_parts()`.
  - Added explicit `fmt_ratio_to(0, 10)` coverage.
  - Added `test_writer_report_has_no_diagnostic_modes`.

Same-failure scan:

- Searched the reporter for shared change table usage after finding the writer
  API/mode label mismatch and split it into reader-specific and writer-specific
  table renderers.

Sensitive data gate:

- Passed locally. Changed durable artifacts contain benchmark paths, benchmark
  configuration keys, and numeric summaries only. No secrets, customer data,
  SNMP communities, bearer tokens, private endpoints, or production log payloads
  were added.

Artifact maintenance gate:

- AGENTS.md: no update needed; this SOW does not change project-wide workflow
  rules.
- Runtime project skills: no update needed; this is
  benchmark tooling documented in `tests/benchmarks/README.md`.
- Specs: no product behavior change; no spec update needed.
- End-user/operator docs: updated `tests/benchmarks/README.md`.
- End-user/operator skills: no output/reference skill exists for benchmark
  reporting.
- SOW lifecycle: this SOW moves to `done/` in the same commit as the
  implementation and documentation.
- SOW-status.md: updated at SOW start and close.

Specs update:

- No product specification update is needed because this SOW changes benchmark
  reporting workflow, not SDK reader/writer behavior or public file contracts.

Project skills update:

- No project skill update is needed before reviewer feedback; the rule is local
  to benchmark reporting and is documented next to the benchmark harnesses.

End-user/operator docs update:

- Updated `tests/benchmarks/README.md`.

End-user/operator skills update:

- No output/reference skill exists for benchmark reporting.

Lessons:

- Shared table helpers can accidentally blur reader and writer dimensions; the
  reporter now keeps reader `mode` and writer `api` labels separate.
- Benchmark report shape tests are cheap enough to keep in-tree; adding them
  prevents manual report regressions in subsequent performance SOWs.

Follow-up mapping:

- No open follow-up remains. Valid reviewer findings were implemented in this
  SOW.

## Outcome

Implemented a stdlib-only benchmark report generator and documented it as the
standard benchmark reporting path. The generator renders reader-core and
writer-core JSON artifacts into a fixed Markdown shape with stable sections,
stable row ordering, explicit `status` columns, before/after deltas, unmatched
row reporting, and explicit conclusion labels.

## Lessons Extracted

- Benchmark presentation is part of the performance contract. If reports are
  hand-shaped, the comparison frame can change without anyone noticing.
- Missing rows must be visible, not silently absent, because absence can mean a
  broken harness, a changed benchmark surface, or an intentional language
  subset.
- Reader and writer reports need separate renderers where their dimensions
  differ; over-shared table helpers make mistakes easy.

## Followup

None.

## Regression Log

None.
