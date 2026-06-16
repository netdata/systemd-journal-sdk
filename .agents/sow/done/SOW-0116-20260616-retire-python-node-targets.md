# SOW-0116 - Retire Python and Node.js product targets

## Status

Status: completed

Sub-state: completed - Python and Node.js are retired from product scope, their tracked implementations are under `experiments/`, active product docs/specs/SOWs/CI now target Rust and Go, and final reviewer findings were resolved or dispositioned.

## Requirements

### Purpose

Refocus this repository on Netdata's required SDK product: Rust and Go systemd journal file readers, writers, helpers, tests, docs, and release surfaces. Python and Node.js are no longer product languages; they become experiments and must not drive product scope, validation gates, docs, release planning, or shared SOW requirements.

### User Request

"Retire both python and node. We don't need them. Netdata only need Rust and Go. I would recommend to move them into a folder called experiments, close any SOW dedicated to them, clean them up from any shared SOW, remove all references from docs and markdown files. They are just noise for the actual product we need and slow us down tremendously."

### Acceptance Criteria

- `python/` and `node/` tracked implementations are moved to `experiments/python/` and `experiments/node/`.
- Product goals, specs, docs, release scope, active/pending SOWs, CI, and validation defaults identify Rust and Go as the required product languages.
- Dedicated Python/Node pending SOWs are closed or superseded.
- Shared active/pending SOWs no longer require Python/Node work.
- Product docs no longer publish Python/Node API pages or language-selection guidance.
- Python remains allowed as a repository test/tooling language for scripts such as docs validators and matrix runners; this does not reintroduce Python SDK product scope.
- Historical completed SOWs and archived evidence are not rewritten except where a status index must point to the new retirement decision.

## Analysis

Evidence reviewed:

- `AGENTS.md:22` currently defines Rust, Go, Node.js, and Python as SDK targets.
- `.agents/sow/specs/product-scope.md:7-12` currently lists all four language targets.
- `README.md:4`, `docs/Home.md`, `docs/Python-API.md`, and `docs/Node-API.md` publish Python/Node consumer docs.
- `.agents/sow/pending/SOW-0113-20260615-nodejs-optional-native-mmap-reader.md` is a dedicated Node.js follow-up SOW.
- `.agents/sow/pending/SOW-0066-20260530-v1-release-and-registry-publication.md` still includes npm and Python package publication scope.
- `.agents/sow/current/SOW-0009-20260523-benchmark-profile-optimize.md` still carries Python/Node benchmark/porting references.
- SOW-0115 repeat review showed Node cannot satisfy the accepted portable identity-helper constraints under the no-subprocess/no-native-addon policy.

Facts:

- Netdata's identified immediate consumers in this repo are Rust NetFlow and Go `snmp_traps`.
- Python/Node SDK parity has become a recurring blocker for performance, host identity, mmap, locking, docs verification, package hygiene, and review scope.
- Keeping Python/Node as required product targets creates mandatory work that does not serve the current product purpose.

Inference:

- The long-term-best path is to retire Python/Node from product scope in one explicit repository cleanup so future SOWs, reviewers, docs, and release gates stop treating them as required.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The repository product contract promises four SDK languages, but the actual Netdata product need is Rust and Go. The extra language targets create noisy validation matrices, docs, CI, release gates, and SOW obligations that slow down Rust/Go work and cannot cleanly satisfy new host-helper requirements.

Evidence reviewed:

- Files listed in Analysis above.
- Tracked file count before move: 63 tracked Node files and 50 tracked Python files. Ignored/generated paths exist under `node/.local`, `node/node_modules`, and Python cache directories; those remain ignored after the move.

Affected contracts and surfaces:

- `AGENTS.md`, `.agents/sow/specs/product-scope.md`, docs under `docs/`, README, CI workflows, docs-example harness expectations, interoperability/benchmark defaults, active/pending SOWs, and status ledgers.

Existing patterns to reuse:

- SOW status/directory consistency rules.
- `experiments/` as a clear non-product boundary.
- Rust/Go-only benchmark examples already present in benchmark docs and scripts.

Risk and blast radius:

- Large repo-scope cleanup. Risk is stale references to Python/Node in docs, specs, CI, or validation defaults.
- Historical completed SOWs may still mention Python/Node as past work; rewriting them would damage history, so they remain historical records.
- Test harness scripts are written in Python; the cleanup must not remove Python as an implementation language for repository tooling.

Sensitive data handling plan:

- No sensitive data involved.

Implementation plan:

1. Move tracked `node/` and `python/` implementations under `experiments/`.
2. Add experiment boundary documentation.
3. Update `AGENTS.md`, `product-scope.md`, README, docs, and CI to Rust/Go product scope.
4. Remove Python/Node API docs and docs navigation entries.
5. Update docs-example and runtime-purity tooling to Rust/Go SDK targets.
6. Update interoperability/benchmark defaults to Rust/Go product targets; leave Python as the scripting language for test harnesses.
7. Close/supersede dedicated pending Python/Node SOWs and clean shared active/pending SOWs.
8. Run SOW audit, docs validation, focused tests for changed tooling, and Rust/Go product checks that are feasible locally.

Validation plan:

- `rg` scans for stale product-target claims in active docs/specs/SOWs.
- `.agents/sow/audit.sh`.
- `python3 tests/docs/check_wiki_docs.py`.
- `python3 tests/docs/test_verify_examples.py`.
- Focused Python unit checks for modified tooling.
- `git diff --check`.

Artifact impact plan:

- `AGENTS.md`: update project language goals and purity rules.
- Runtime project skills: update only if their how-to rules still require retired language targets.
- Specs: update `product-scope.md` and any active spec references.
- End-user/operator docs: remove Python/Node pages and language references.
- End-user/operator skills: none expected.
- SOW lifecycle: add this SOW, close/supersede SOW-0113, update SOW-0115 and shared pending/current SOWs.
- SOW status ledgers: update canonical and root status files.

Open decisions:

- None. User accepted retiring Python and Node.js as product targets and moving them under `experiments/`.

## Implications And Decisions

### Decision 1 - Python and Node.js product retirement

- **User decision 2026-06-16: 1A accepted.**
- **1A. Retire Python and Node.js as product targets (ACCEPTED, long-term-best).** Move implementations under `experiments/`, remove them from product docs/specs/SOW scope, and stop requiring parity/release/validation gates for them. Pros: aligns the repo with Netdata's Rust/Go needs, reduces validation and review drag, avoids ongoing Node/Python platform restrictions. Cons: breaking scope change for anyone treating Python/Node as product SDKs. Risk: stale references if cleanup is incomplete.
- **1B. Keep Python/Node as maintenance-only product surfaces.** Rejected because it preserves docs, tests, and release obligations.
- **1C. Keep only Python or only Node.** Rejected because neither is required by the identified Netdata consumers.

## Plan

1. Create SOW and record the user decision.
2. Move tracked implementation trees to `experiments/`.
3. Clean product docs/specs/CI/tooling.
4. Close/supersede dedicated SOWs and clean shared SOWs.
5. Validate and record evidence.

## Delegation Plan

Implementation is performed in this session because the user directly requested the scope cleanup and it is primarily repository hygiene, docs/specs, and validation routing. External reviewers may be run after the cleanup if requested or if local validation exposes uncertainty.

## Execution Log

### 2026-06-16

- SOW created and user decision recorded.
- Moved tracked `node/` and `python/` implementation trees under `experiments/node/` and `experiments/python/`.
- Added `experiments/README.md` plus retired-experiment notices in the moved implementation READMEs.
- Removed published Python and Node API wiki pages and removed them from wiki navigation.
- Updated `AGENTS.md`, `README.md`, product specs, docs, CI workflows, docs-example tooling, interoperability runners, runtime-purity checks, and benchmark/reporting defaults so Rust and Go are the only product SDK targets.
- Updated runtime project skills: `project-docs-authoring` now defines verified examples for Rust/Go only; `project-journal-compatibility` now describes Rust/Go product compatibility matrices.
- Closed dedicated pending SOW-0113 without implementation and moved it to `.agents/sow/done/`; updated canonical and root SOW status ledgers.
- Cleaned SOW-0115 so the portable identity-helper SOW no longer has retired-language helper scope as an open blocker.
- Replaced retired-SDK product test dependencies with a generated Go live fixture writer in `tests/interoperability/go_fixture_writer.py` and stock `journalctl` fixture inspection where needed.
- Fixed generated Go helper modules in docs and interoperability tooling to mirror the Go SDK module's direct requirements and copied `go.sum`, so temporary helpers build cleanly with the local `replace`.
- Removed optional Python/Node experiment peer execution from the active Netdata function comparators so the active product harnesses compare Rust/Go SDK behavior against the installed plugin only.
- Removed stale retired-language discrepancy codes from the systemd matrix runner.
- External review found stale retired-language metadata references in `.gitignore`, `PROVENANCE.md`, and the Codacy summary fixture tests; fixed them by moving ignore/provenance paths under `experiments/` and changing synthetic code-scanning fixtures to Go paths.
- External review found that `tests/interoperability/run_compact_matrix.py`, `tests/interoperability/run_compression_matrix.py`, and `tests/interoperability/run_mixed_directory_matrix.py` still called `sys.exit(main())` after `import sys` had been removed during retired Python SDK path cleanup. Restored `import sys` in all three scripts.
- Repeat external review found one stale consumer-doc product claim in `docs/Explorer-And-Netdata-Queries.md` saying "All four SDKs"; changed it to Rust and Go product SDK wording.

## Validation

Acceptance criteria evidence:

- `node/` and `python/` tracked files are now under `experiments/node/` and `experiments/python/`.
- Product docs under `docs/` now publish Rust and Go SDK/API guidance only; Python/Node API pages were removed.
- CI product-language workflows now target Rust/Go for docs examples, coverage, and CodeQL. Codacy still uses Node/Python as analysis tooling only.
- Active/pending SOWs were updated: SOW-0113 is closed; SOW-0115 and SOW-0066 now target Rust/Go product scope.
- Historical completed SOWs and status-ledger historical notes were preserved as history, with SOW-0113's current state corrected.

Tests or equivalent validation:

- `python3 tests/docs/check_wiki_docs.py` - passed, validated 15 wiki markdown files.
- `python3 tests/docs/test_verify_examples.py` - passed, 50 tests.
- `python3 tests/docs/test_check_wiki_docs.py` - passed, 27 tests.
- `python3 tests/docs/verify_examples.py` - passed, 31/31 Rust/Go examples.
- `python3 tests/runtime_purity/test_core_runtime_purity.py` - passed, 2 tests.
- `python3 tests/interoperability/run_journalctl_query_matrix.py` - passed, stock/Go/Rust query and follow matrix on `systemd 260 (260.1-2-manjaro)`.
- `.local/python-venv/bin/python3 tests/interoperability/run_compact_matrix.py` - passed, 20/20 compact-format checks for Go/Rust writers and stock/Go/Rust readers.
- `.local/python-venv/bin/python3 tests/interoperability/run_compression_matrix.py` - passed, 24/24 DATA-compression checks for Go/Rust writers and stock/Go/Rust readers.
- `.local/python-venv/bin/python3 tests/interoperability/run_mixed_directory_matrix.py` - passed, 42/42 mixed-directory checks for stock/Go/Rust readers.
- `.local/python-venv/bin/python3 tests/interoperability/run_lock_matrix.py` - passed, Go/Rust contention and stale-lock cases.
- `.local/python-venv/bin/python3 tests/interoperability/run_verify_matrix.py` - passed, including sealed positive/negative verification through the Go fixture writer and stock/Go/Rust verification paths.
- `.local/python-venv/bin/python3 tests/interoperability/run_directory_matrix.py` - passed, stock/Go/Rust directory traversal matrix.
- `.local/python-venv/bin/python3 tests/interoperability/run_binary_matrix.py` - passed, 18/18 binary-field matrix checks. The first attempt was run in parallel with other matrices and hit `.local/interoperability/bin/rust-journalctl` `Text file busy`; rerunning alone passed.
- `.local/python-venv/bin/python3 tests/interoperability/run_live_matrix.py` - passed, 18/18 live feature matrix checks for Go/Rust writers and stock/libsystemd/Go/Rust readers.
- `python3 tests/interoperability/run_byte_identity.py --final-state offline` - passed; systemd/Rust/Go outputs were byte-identical for the validated chain.
- `python3 tests/datasets/ingesters/run_dataset_ingesters.py --language go --final-state offline` - passed; Go ingester output verified with journal tooling.
- `tests/coverage/upload_codacy_coverage.sh` without `CODACY_API_TOKEN` - skipped as expected after checking only Rust/Go coverage report inputs.
- `bash -n tests/coverage/upload_codacy_coverage.sh tests/coverage/lib.sh tests/coverage/run_go_coverage.sh tests/coverage/run_rust_coverage.sh` - passed.
- `python3 tests/netdata_function/test_stateful_function_compare.py` - passed, 19 tests.
- `python3 tests/netdata_function/test_compare_function_json.py` - passed, 51 tests.
- `python3 tests/systemd_matrix/run_systemd_matrix.py --help` - passed.
- `python3 tests/vm_matrix/run_vm_matrix.py --help` - passed.
- `python3 tests/corpus_eval/run_selective_real_corpus.py --help` - passed.
- `python3 tests/datasets/ingesters/run_dataset_ingesters.py --help` - passed.
- `python3 tests/netdata_function/run_function_compare.py --help` - passed and no longer advertises retired SDK peers.
- `python3 tests/netdata_function/run_stateful_function_compare.py --help` - passed and now describes Go-SDK fixture generation.
- `python3 -m json.tool tests/corpus_eval/reports/selective-real-corpus-report.json` - passed.
- `python3 -m py_compile ...` for modified docs, interoperability, runtime-purity, benchmark, dataset, systemd, VM, corpus, coverage-adjacent, and Netdata harness scripts - passed.
- `git diff --check` - passed.
- `.agents/sow/audit.sh` - passed.

Real-use evidence:

- The docs harness built synthetic fixtures through the Go SDK and compiled/ran all published Rust/Go examples against them.
- The journalctl query matrix built Go/Rust journalctl binaries, generated repo-local fixtures through the Go writer, and verified stock/Go/Rust file and directory query/follow behavior.
- The Netdata stateful fixture generator now writes fixtures through the Go SDK and validates fixture content through stock `journalctl`, not through a retired SDK.

Reviewer findings:

- First external reviewer batch after the initial cleanup was not clean. Findings included:
  - Codacy coverage upload still expected `coverage-node` and `coverage-python` reports.
  - Some secondary matrix/corpus/Netdata harnesses still referenced root `python/` or `node/` paths or retired SDK roles.
  - Netdata stateful fixture generation and tests still depended on the retired Python SDK.
- Disposition:
  - Removed retired coverage report inputs and deleted the retired coverage scripts.
  - Removed active root `python/` and `node/` paths from systemd, VM, corpus, dataset, benchmark, docs, runtime-purity, and Netdata product harnesses.
  - Replaced retired SDK fixture generation with the Go fixture writer and stock `journalctl` inspection.
  - Removed optional Python/Node experiment peer execution from active Netdata function comparators.
- Second external reviewer batch after the broad cleanup found low-risk stale metadata/test references:
  - `.gitignore` still ignored the old `python/*.egg-info/` path.
  - `PROVENANCE.md` still named `node/vendor/node-liblzma-wasm/` and the published Node.js package.
  - `tests/code_scanning/summarize_findings.py` and its test still used retired root `node/`/`python/` classifier fixtures.
- Disposition:
  - Updated `.gitignore` to `experiments/python/*.egg-info/`.
  - Updated provenance to `experiments/node/vendor/node-liblzma-wasm/` and "retired Node.js experiment package".
  - Replaced retired root path-class prefixes with `experiments/` and changed synthetic Codacy fixture paths to Go files.
- GLM reviewer then found a real runtime regression in `run_compact_matrix.py`, `run_compression_matrix.py`, and `run_mixed_directory_matrix.py`: `py_compile` passed but the scripts crashed at module exit because `sys.exit(main())` remained after `import sys` was removed.
- Disposition:
  - Restored `import sys` in all three scripts.
  - Ran all three scripts end-to-end successfully and added the runs above to Validation.
- A later reviewer noted that the new lesson should apply to all substantively rewritten executable interoperability scripts, especially `run_verify_matrix.py` because it exercises the new sealed Go fixture-writer path.
- Disposition:
  - Ran and recorded `run_verify_matrix.py`, `run_directory_matrix.py`, `run_binary_matrix.py`, and `run_live_matrix.py` end-to-end.
- Final external reviewer batch after the Explorer-doc and matrix-validation fixes returned 6/7 `READY TO COMPLETE`. One reviewer voted `NOT READY TO COMPLETE` only because new SOW/helper files were still untracked before commit preparation.
- Disposition:
  - The untracked-file issue is a close-commit staging requirement, not a product or validation defect.
  - Closing this SOW stages the new SOW/helper files explicitly by path, per the repository git rules.

Same-failure scan:

- Strict stale-path scan over `tests`, `.agents`, `README.md`, `AGENTS.md`, `docs`, `.github`, and status ledgers, excluding `experiments/`, `.local/`, and completed SOWs, now finds only historical status ledger text and this SOW's own explanation.
- Broader active-surface scan for Python/Node terms now finds only Codacy tooling, Python test-harness language, shell `set -euo pipefail` false positives, JSON/Python tooling comments, and historical/status text.
- Product docs, specs, active/pending SOWs, CI product workflows, and active validation defaults no longer require retired SDKs.
- Same-failure scan for modified active Python scripts using `sys.` without `import sys`, excluding `experiments/`, found no remaining active-surface misses.
- Same-failure scan for stale multi-language product wording in `docs/`, specs, `AGENTS.md`, and `README.md` found the Explorer doc miss above and one ambiguous spec phrase; after the fixes it no longer finds product-doc "all four SDKs" wording outside historical status text, and the remaining `AGENTS.md` hit describes runtime architecture layers rather than language targets.

Sensitive data gate:

- No raw secrets, credentials, bearer tokens, SNMP communities, customer/community personal data, customer-identifying IPs, private endpoints, or proprietary incident details were introduced.

Artifact maintenance gate:

- AGENTS.md: updated Rust/Go project goal and product target wording.
- Runtime project skills: updated `.agents/skills/project-docs-authoring/SKILL.md` and `.agents/skills/project-journal-compatibility/SKILL.md`.
- Specs: updated `.agents/sow/specs/product-scope.md` and `.agents/sow/specs/systemd-journal-plugin-facets.md`.
- End-user/operator docs: updated `README.md` and `docs/`; removed `docs/Python-API.md` and `docs/Node-API.md`.
- End-user/operator skills: none exist for this repository.
- SOW lifecycle: added SOW-0116, closed SOW-0113, kept SOW-0009 current/paused, kept SOW-0115 pending/open with Rust/Go scope.
- SOW-status.md: updated `.agents/sow/SOW-status.md` and root `SOW-status.md`.

Specs update:

- Product scope now states Rust/Go as product SDK targets and records retired implementations as non-product experiments.

Project skills update:

- Docs and journal compatibility skills now describe Rust/Go-only product validation and compatibility expectations.

End-user/operator docs update:

- Consumer wiki pages now route users to Rust and Go APIs only.

End-user/operator skills update:

- No output/reference skills are affected.

Lessons:

- Generated temporary Go modules that import the local SDK via `replace` must mirror direct Go SDK requirements and copy `go.sum`; otherwise builds fail under a clean module cache.
- For executable Python harness rewrites, `py_compile` is not enough evidence: it does not catch runtime name resolution failures such as removing `import sys` while keeping `sys.exit(main())`. Rewritten matrix scripts need at least `--help` or end-to-end execution evidence.

Follow-up mapping:

- No new follow-up SOW is required from local validation. Retired-SDK runtime dependencies found in product tests were replaced with Go SDK fixture writing, stock journal inspection, or removed from active product harnesses.

## Outcome

Completed. Python and Node.js are retired from this repository's product SDK
scope and preserved only as non-product experiments under `experiments/`.
Rust and Go are now the only product SDK targets in active docs, specs, CI,
validation defaults, and active/pending SOW scope.

## Lessons Extracted

Generated helper modules need complete local Go module metadata, not just a `replace` directive, when they compile SDK packages with third-party imports.

Executable Python harness rewrites need runtime execution evidence, not only `py_compile`, because syntax compilation will not catch unresolved names such as a removed `import sys` with a remaining `sys.exit(main())`.

## Followup

No new follow-up SOW is required for SOW-0116. Reviewer observations about
optional Codacy `experiments/` exclusion and cosmetic wording are non-blocking;
they do not change the product contract or close outcome.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and later testing or use found broken behavior. Use a dated `## Regression - YYYY-MM-DD` heading at the end of the file. Never prepend regression content above the original SOW narrative.
