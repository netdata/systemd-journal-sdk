# SOW-0018 - Compact Journal Format

## Status

Status: completed

Sub-state: compact reader/writer support implemented, validated, reviewed, and ready to move to `.agents/sow/done/`.

## Requirements

### Purpose

Add systemd compact journal format support where applicable, with shared reader/writer validation and explicit compatibility evidence.

### User Request

The user requires pure SDKs that read and write journal files according to systemd journal rules. SOW-0008 confirmed all current writers create regular non-compact journals; compact format remains a final writer-target gap.

### Assistant Understanding

Facts:

- Current Rust, Go, Node.js, and Python writers create regular, non-compact journal files.
- Current accepted reader slices either reject compact journals or do not claim compact support.
- Compact journals change object layout and offset widths, so they are not a small flag-only feature.
- User decision on 2026-05-25: compact format support is required, and writers must expose an option allowing callers to choose regular or compact output.

Inferences:

- Reader support and fixture inventory should come before writer support.
- Byte-identical compact output may need to wait for deterministic dataset and ingester work.

Unknowns:

- Exact compact fixture coverage available in systemd v260.1 and whether additional generated fixtures are required.

### Acceptance Criteria

- A systemd compact-format reference inventory records exact object layout, header flags, reader behavior, writer behavior, and tests/fixtures used.
- Readers in Rust, Go, Node.js, and Python handle compact journals or return controlled unsupported errors until implementation is complete.
- Writers can emit compact journals only after all reader and stock-tool compatibility gates pass.
- Shared fixtures and interoperability tests cover compact journals across every language.
- Stock `journalctl --verify --file`, stock reads, stock libsystemd reads, and repository readers pass for compact files written by repository writers.
- Writer APIs in Rust, Go, Node.js, and Python expose an explicit caller option for regular vs compact output; the default must remain regular unless a SOW records a user decision to change defaults.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/current/SOW-0008-20260523-interoperability-and-full-writer-features.md`
- `.agents/sow/specs/product-scope.md`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
- `src/libsystemd/sd-journal/journal-def.h`
- `test/journal-data/`
- `test/test-journals/`

Current state:

- Regular non-compact journal support is the current cross-language writer/read surface.
- Compact format is listed as not implemented in product scope and interoperability docs.

Risks:

- Compact layout changes can create silent reader misparsing if treated as a minor variant.
- Writer support before reader support can produce files that repository tooling cannot diagnose.
- Byte identity may expose allocation/layout deltas after semantic compatibility is achieved.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The final writer target includes regular and compact journal formats where applicable, but current SDKs only support regular journals. Compact support needs a format inventory and staged reader/writer rollout.

Evidence reviewed:

- Product scope says current writers are regular, non-compact.
- SOW-0008 records compact format as an open writer feature gap.
- systemd journal definitions include compact-related object layout mechanics that need direct reference inventory.

Affected contracts and surfaces:

- Journal header flags and object layout parsing.
- Reader iteration, matching, export, JSON, and cursor behavior.
- Writer object construction and offset arrays.
- Interoperability and conformance fixtures.
- File-backed journalctl behavior.

Existing patterns to reuse:

- Systemd test inventory approach from SOW-0003.
- Shared conformance manifests and fixture runners.
- Interoperability matrix structure from SOW-0008.

Risk and blast radius:

- High. Compact layout affects core parser and writer object code in all languages.

Sensitive data handling plan:

- Use upstream fixtures and synthetic generated files only. Record upstream paths and commits, not workstation paths. No sensitive runtime data expected.

Implementation plan:

1. Inventory compact format from systemd source, docs, and fixtures.
2. Add compact fixture coverage and controlled unsupported behavior tests.
3. Implement reader support per language.
4. Implement writer support only after reader support is verified.
5. Add compact interoperability matrix and update docs/specs.

Validation plan:

- Compact fixture tests per language.
- Cross-language compact writer/reader matrix.
- Stock journalctl/libsystemd verification for generated compact files.
- Existing regular, binary, compression, live, and lock matrices remain passing.
- External reviewers inspect parser/writer blast radius.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: update only if compact-specific validation becomes durable workflow.
- Specs: update product scope with exact compact support state.
- End-user/operator docs: update README feature matrices.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: track activation, implementation, review, validation, and completion in one SOW unless scope grows beyond compact format support.
- SOW-status.md: update when activated or closed.

Open-source reference evidence:

- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
- `src/libsystemd/sd-journal/journal-def.h`
- `test/journal-data/`
- `test/test-journals/`

Open decisions:

- None blocking activation. If compact writer support proves too broad, split reader and writer SOWs with evidence.

## Implications And Decisions

1. Reader-before-writer compact rollout
   - Decision: compact reader support and fixtures must precede compact writer claims.
   - Reason: writers that produce files repository readers cannot inspect would weaken the project debugging surface.
   - Risk: compact writer delivery may need multiple chunks.

2. Caller-selectable writer format
   - Decision: compact journal support is required, and every writer must expose an option allowing callers to choose regular or compact output.
   - Reason: compact format has a different on-disk layout and a 4 GiB file-size ceiling, so it is not safe as an implicit or silent default change.
   - Implication: writer options, directory writer propagation, livewriter/ingester fixture flags, and docs must distinguish regular vs compact output.
   - Risk: if defaults change accidentally, existing byte-identical regular writer validation and downstream expectations can regress.

## Plan

1. Inventory systemd compact format and fixtures.
2. Implement reader handling and conformance fixtures.
3. Implement writer support.
4. Extend interoperability matrices.
5. Review, validate, and update specs/docs.

## Delegation Plan

Implementer:

- Preferred implementer is `llm-netdata-cloud/kimi-k2.6`.

Reviewers:

- Reviewer pool is `llm-netdata-cloud/minimax-m2.7-coder`, `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`, and `llm-netdata-cloud/glm-5.1`.

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

- Record implementer failure, reviewer failure, audit failure, fixture gaps, or model unavailability before changing plan or model.
- External-agent repository-boundary violations must stop the run immediately. Record the command class, side effect, and replacement implementer before continuing.

## Execution Log

### 2026-05-25

- Activated after SOW-0021 completed and was pushed.
- Recorded user decision: compact format is required, and writers must expose explicit regular/compact selection while preserving regular output as the default unless changed by a separate user decision.
- Updated delegation metadata to use Kimi as implementer and Minimax as reviewer-only per current project routing.
- Stopped the first Kimi implementer run after it violated the repository boundary by running a live-host journal write command (`systemd-cat`) for compact probing. Side effect: a synthetic test entry was written to the workstation journal outside this repository. No repository files were changed by that run.
- Recorded the boundary failure before switching implementer routing. Next implementer is `llm-netdata-cloud/qwen3.6-plus`, following the fallback hierarchy.
- Stopped the Qwen implementer run after it repeated the repository-boundary violation by running `SYSTEMD_JOURNAL_COMPACT=1 systemd-cat ...`. Side effect: a second synthetic test entry was written to the workstation journal outside this repository. No repository files were changed by that run.
- External implementer routing is paused for this SOW until implementation can proceed without allowing live-host journal commands. Any next implementation pass must either run locally under direct project-manager control or use a patch-only/no-command delegation mode.
- Implemented compact-format reader and writer support locally for Go, Rust, Node.js, and Python after the external-agent boundary failures. Scope included explicit writer options, directory writer propagation, livewriter/dataset ingester `--compact` flags, compact DATA payload offset handling, compact ENTRY/ENTRY_ARRAY item sizing, compact DATA tail fields, and compact 32-bit offset ceiling checks.
- Added `tests/interoperability/run_compact_matrix.py` to validate compact layout plus stock/repository reader interoperability across all writers.
- Updated product scope, language READMEs, interoperability docs, and SOW status to describe compact support as implemented while keeping regular output as the default.
- First reviewer round:
  - Mimo: `PRODUCTION GRADE: YES`; raised only residual risks around 4 GiB boundary coverage, compact+compression combination coverage, and legacy `jf` drift.
  - GLM: `PRODUCTION GRADE: YES`; raised maintainability notes about compact DATA tail offsets being hardcoded in Go/Node.js/Python.
  - Minimax: `PRODUCTION GRADE: NO` because it believed Rust rotation dropped compact mode. Disposition: false blocker; `JournalFile::create_successor()` preserves `HeaderIncompatibleFlags::Compact` from the old file header. Cleanup still removed the misleading unused rotation parameter and added a compact rotation test.
  - Qwen: stopped after roughly 10 minutes with no output; no repository changes were made by that reviewer run.
- Addressed first-round findings:
  - Added named compact DATA tail offset constants in Go, Node.js, and Python.
  - Added a Rust `journal-log-writer` compact rotation regression test.
  - Aligned legacy `jf` compact u32 offset assertions and removed the dead stored `boot_id` field while preserving constructor API shape.
- Second reviewer round:
  - Minimax: `PRODUCTION GRADE: YES`; raised one Python dataset ingester rejection-mode concern that was disproven by `python/cmd/dataset_ingester.py` passing `compact` through `ingest_rejections()` to `make_writer()`.
  - Mimo: `PRODUCTION GRADE: YES`; raised only low-risk observations about assert-style internal invariants, 4 GiB boundary materialization, and compact+compression coverage.
  - GLM rerun: stopped after several minutes with no findings or final output; no repository changes were made by that reviewer run.
- Extended `run_compact_matrix.py` with `--compression` and `--compression-threshold-bytes`, then validated compact journals with uncompressed, zstd, xz, and lz4 DATA objects.

## Validation

Complete.

Acceptance criteria evidence:

- Writer APIs expose explicit compact selection:
  - Go: `journal.Options.Compact`.
  - Rust: `JournalFileOptions::with_compact(true)` and `journal::Config::with_compact(true)`.
  - Node.js: `compact: true` and `format: 'compact'`.
  - Python: `compact: True` and `format: 'compact'`.
- Regular output remains the default in every writer.
- Compact files written by Go, Rust, Node.js, and Python set `HEADER_INCOMPATIBLE_COMPACT`, use compact ENTRY and ENTRY_ARRAY offsets, use the compact DATA payload offset, and are readable by stock `journalctl --file`, stock libsystemd, and all repository journalctl implementations.
- Compact+compression files written by Go, Rust, Node.js, and Python pass the same stock and repository reader matrix for zstd, xz, and lz4 DATA object compression.
- Rust directory writer rotation preserves compact mode, covered by `test_compact_rotation_preserves_compact_format`.
- Repository boundary check: all implementation and validation writes stayed inside this repository after the recorded external implementer boundary failures. The only durable generated artifacts are source/docs/SOW changes; runtime artifacts stayed under `.local/`.

Tests or equivalent validation:

- `cd go && GOMODCACHE=$PWD/../.local/go/pkg/mod GOPATH=$PWD/../.local/go GOCACHE=$PWD/../.local/go-build go test ./...` - PASS.
- `cd rust && CARGO_HOME=$PWD/../.local/cargo CARGO_TARGET_DIR=$PWD/../.local/rust-target cargo test -p journal-core -p journal-log-writer -p livewriter -p dataset_ingester -p journal_file` - PASS.
- `cd node && npm_config_cache=$PWD/../.local/npm-cache node test/all.js` - PASS.
- `cd python && PYTHONPATH=$PWD:../.local/python-deps python3 test_all.py` - PASS.
- `PYTHONPATH=$PWD/python:$PWD/.local/python-deps python3 tests/interoperability/run_compact_matrix.py --entries 10 --compression none` - PASS, 56/56 checks.
- `PYTHONPATH=$PWD/python:$PWD/.local/python-deps python3 tests/interoperability/run_compact_matrix.py --entries 10 --compression zstd --compression-threshold-bytes 1` - PASS, 56/56 checks.
- `PYTHONPATH=$PWD/python:$PWD/.local/python-deps python3 tests/interoperability/run_compact_matrix.py --entries 10 --compression xz --compression-threshold-bytes 1` - PASS, 56/56 checks.
- `PYTHONPATH=$PWD/python:$PWD/.local/python-deps python3 tests/interoperability/run_compact_matrix.py --entries 10 --compression lz4 --compression-threshold-bytes 1` - PASS, 56/56 checks.
- `GOMODCACHE=$PWD/.local/go/pkg/mod GOPATH=$PWD/.local/go GOCACHE=$PWD/.local/go-build CARGO_HOME=$PWD/.local/cargo-home PYTHONPATH=$PWD/python:$PWD/.local/python-deps npm_config_cache=$PWD/.local/npm-cache python3 tests/interoperability/run_matrix.py --entries 10` - PASS, 104/104 checks.
- `.agents/sow/audit.sh` - PASS.
- `git diff --check` - PASS.
- SOW close-term scan found no unmapped work items; the only close-term match is `Status: completed`.

Real-use evidence:

- Stock systemd used for validation: `systemd 260 (260.1-2-manjaro)`.
- Compact matrix used stock `journalctl --verify --file` and stock `journalctl --file` against every compact writer output for uncompressed, zstd, xz, and lz4 compact fixtures.
- Compact matrix used the stock libsystemd helper `tests/conformance/binary/libsystemd_binary_field_reader.c` against every compact writer output for uncompressed, zstd, xz, and lz4 compact fixtures.

Reviewer findings:

- First-round Mimo and GLM returned `PRODUCTION GRADE: YES`.
- First-round Minimax returned `PRODUCTION GRADE: NO` for a Rust rotation blocker. Disposition: false positive; `rust/src/crates/journal-core/src/file/file.rs` `create_successor()` propagates compact from `HeaderIncompatibleFlags::Compact`. Cleanup removed the confusing unused parameter and added direct compact rotation test coverage.
- First-round Qwen was unresponsive and was stopped with no file changes.
- Second-round Minimax returned `PRODUCTION GRADE: YES`. Its Python dataset ingester concern was false; `python/cmd/dataset_ingester.py` passes `compact` through rejection mode.
- Second-round Mimo returned `PRODUCTION GRADE: YES`; residual notes were dispositioned as internal invariant style or were covered by the new compact+compression matrix runs.
- Second-round GLM was unresponsive and was stopped with no file changes.

Same-failure scan:

- Checked compact option propagation in every dataset ingester and livewriter:
  - Go `go/internal/testcmd/dataset_ingester/main.go`.
  - Rust `rust/src/internal/testcmd/dataset_ingester/src/main.rs`.
  - Node.js `node/cmd/dataset_ingester.js`.
  - Python `python/cmd/dataset_ingester.py`.
- Checked compact DATA tail field writes across Go, Node.js, and Python after replacing magic offsets with named constants.
- Checked legacy and primary Rust compact u32 offset assertions after aligning `jf` and `journal-core`.

Sensitive data gate:

- Clean. Durable artifacts contain public upstream references, synthetic fixture names, command evidence, and source/docs only. No raw secrets, credentials, private endpoints, customer identifiers, or personal data were added.

Artifact maintenance gate:

- AGENTS.md: no update needed; compact support does not change project-wide workflow or guardrails.
- Runtime project skills: no update needed; `project-journal-compatibility` already requires shared interoperability and stock reader validation for writer changes.
- Specs: `product-scope.md` updated with exact shipped compact reader/writer support state.
- End-user/operator docs: Go, Rust, Node.js, Python READMEs and `tests/interoperability/README.md` updated.
- End-user/operator skills: none exist for this project.
- SOW lifecycle: status set to `completed`; file will be moved to `.agents/sow/done/` before commit.
- SOW-status.md: updated on activation and will be updated on close.

Specs update:

- `product-scope.md` updated to record compact reader/writer support for every language, regular output as the default, explicit compact writer options, and compact matrix validation.

## Outcome

Completed.

Rust, Go, Node.js, and Python now read and write systemd compact journal files through explicit writer options while preserving regular output as the default. The compact layout supports binary fields, DATA tail fields, compact ENTRY and ENTRY_ARRAY offsets, reopen behavior, directory writer propagation, and Rust rotation preservation. Stock `journalctl --file`, stock libsystemd, and all repository readers validate every compact writer output, including uncompressed, zstd, xz, and lz4 DATA object variants.

## Lessons Extracted

- Compact support is not a flag-only feature; tests must inspect the object layout and run stock readers.
- External implementer prompts for journal work must continue to explicitly forbid live-host journal commands; both failed implementer attempts repeated this class of boundary violation.
- Reviewer concerns around rotation are easier to settle with a direct integration test than with explanation alone.
- Compact+compression should stay part of the compact matrix because it verifies the DATA payload offset and decompression paths together.

## Followup

None.

## Regression Log

None yet.
