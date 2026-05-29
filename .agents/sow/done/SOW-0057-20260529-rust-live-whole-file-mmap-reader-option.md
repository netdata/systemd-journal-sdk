# SOW-0057 - Rust Live Whole-File Mmap Reader Option

## Status

Status: completed

Sub-state: completed. Rust live readers can explicitly opt into whole-file
mmap for measurement; defaults remain live/windowed, and benchmark evidence
shows whole-file mmap does not explain the Go/Rust reader gap on the current
compact/offline payload corpus.

## Requirements

### Purpose

Measure whether Rust reader throughput is being limited by the current
live/windowed mmap strategy, so Rust can be compared fairly against the Go
reader's mmap-backed hot path before deciding what optimizations to keep.

### User Request

The user asked to add the Rust option needed to measure live whole-file mmap
after the analysis found that current benchmarks compare Rust live/windowed
against Go live/whole-file mmap.

### Assistant Understanding

Facts:

- The Rust reader `ReaderOptions` already contains an
  `ExperimentalMmapStrategy`, but live readers currently route through
  `JournalFile::open_path()` and effectively use windowed mmap.
- The benchmark harness already accepts `--mmap-strategy whole-file`, but Rust
  live reader cases do not currently exercise it.
- Go reader mmap mode maps the whole file by default on Unix.

Inferences:

- The least risky implementation is to make the existing Rust reader strategy
  option effective for live readers, without changing defaults.
- This option should stay explicitly experimental until live-reader safety,
  memory-pressure, and performance tradeoffs are validated.

Unknowns:

- Whether live whole-file mmap improves Rust reader throughput on the current
  benchmark corpus.
- Whether the gain, if any, is worth the extra virtual-memory pressure for
  production consumers.

### Acceptance Criteria

- Rust live readers can opt into whole-file mmap through the existing
  `ReaderOptions::with_mmap_strategy(ExperimentalMmapStrategy::WholeFile)`.
- Default Rust live readers remain windowed.
- Reader-core benchmarks include Rust live/whole-file cases for single-file and
  open-files payload/facade modes.
- Targeted Rust tests prove live/whole-file maps the whole file and remaps only
  when a later access requires newly appended bytes.
- Benchmarks record Rust live/windowed versus live/whole-file results.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/done/SOW-0052-20260529-rust-reader-last-mile-optimization.md`
- `.agents/sow/done/SOW-0056-20260529-go-reader-hot-path-optimization-phase2.md`
- `rust/src/journal/src/lib.rs`
- `rust/src/crates/journal-core/src/file/mmap.rs`
- `rust/src/crates/journal-core/src/file/file.rs`
- `tests/benchmarks/run_reader_core_benchmarks.py`

Current state:

- `ReaderOptions` defaults to live/windowed and exposes
  `with_mmap_strategy()`.
- `open_journal_file()` sends `ReaderBounds::Live` through
  `JournalFile::open_path()`, which constructs a live `WindowManager` with
  `ExperimentalMmapStrategy::Windowed`.
- `WindowManager::new_with_bounds_mode()` currently forces non-snapshot,
  non-writer-owned readers to windowed strategy.
- The benchmark matrix has Rust live/windowed and Rust snapshot/whole-file
  cases, but no Rust live/whole-file cases.

Risks:

- Whole-file mmap increases virtual address usage and may be inappropriate for
  large live files on constrained systems.
- Live files can grow. Remapping must not happen while a reader-owned borrowed
  slice is live.
- This option must not change default behavior or production claims until it is
  measured and compatibility-validated.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Current Go/Rust reader comparison is not apples-to-apples because Go live mmap
  maps the whole file, while Rust live readers are forced to windowed mmap. The
  first fix is a measurement option, not a default behavior change.

Evidence reviewed:

- `rust/src/journal/src/lib.rs` shows `ReaderOptions` has a mmap strategy but
  live opens use `JournalFile::open_path()`.
- `rust/src/crates/journal-core/src/file/mmap.rs` shows live readers are forced
  to windowed strategy in `new_with_bounds_mode()`.
- `tests/benchmarks/run_reader_core_benchmarks.py` shows benchmark cases do not
  include Rust live/whole-file modes.
- `.local/benchmarks/reader-core-go-phase2-final/20260529T125459Z/summary.json`
  shows current reported Go/Rust payload comparison uses Go live/mmap and Rust
  live/windowed.

Affected contracts and surfaces:

- Rust reader options and internal mmap strategy behavior.
- Rust file and directory reader benchmark cases.
- Rust live-reader memory-mapping behavior when callers explicitly opt in.

Existing patterns to reuse:

- Existing `ExperimentalMmapStrategy::{Windowed, WholeFile}` enum.
- Existing snapshot whole-file tests in `mmap.rs`.
- Existing reader-core benchmark strategy field.

Risk and blast radius:

- Medium. The default stays unchanged, but an explicit live/whole-file option
  touches mmap remap behavior and reader-owned slice lifetime rules.

Sensitive data handling plan:

- Use generated fixtures and benchmark artifacts only.
- Do not record real logs, SNMP communities, credentials, bearer tokens,
  customer data, personal data, private endpoints, or production incident
  details.

Implementation plan:

1. Add a live `WindowManager` constructor that accepts
   `ExperimentalMmapStrategy`.
2. Add Rust `JournalFile` live-open helpers that pass the strategy through.
3. Route `ReaderBounds::Live` through the strategy-aware open path.
4. Add Rust live/whole-file benchmark cases.
5. Add targeted mmap tests for live/whole-file mapping and growth remap.

Validation plan:

- Targeted Rust mmap tests.
- Rust reader-core benchmark cases for live/windowed versus live/whole-file.
- `cargo test` for affected Rust crates or package targets.
- `git diff --check`.
- `.agents/sow/audit.sh`.

Artifact impact plan:

- AGENTS.md: no update expected; project workflow does not change.
- Runtime project skills: no update expected; compatibility workflow does not
  change.
- Specs: update only if the option is documented as a public reader behavior.
- End-user/operator docs: update Rust README only if the option becomes a
  documented public option beyond benchmark measurement.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: current follow-up SOW; close after validation and benchmark
  evidence.
- SOW-status.md: update current status when this SOW is opened and closed.

Open-source reference evidence:

- No external open-source checkout was needed. This is an SDK-local measurement
  option around an existing SDK enum and benchmark harness.

Open decisions:

- User decision recorded below: add the option now for measurement while keeping
  defaults unchanged.

## Implications And Decisions

1. 2026-05-29 Rust live whole-file measurement option
   - Decision: add the option to Rust so live/whole-file mmap can be measured.
   - Implication: benchmark comparisons can separate mmap strategy from
     language/runtime differences.
   - Risk: whole-file live mmap may increase virtual-memory pressure; it is not
     made the default in this SOW.

## Plan

1. Implement explicit live/whole-file mmap support in Rust internals.
2. Add benchmark matrix entries for Rust live/whole-file payload/facade modes.
3. Validate with targeted Rust tests and benchmark runs.
4. Record benchmark results and close the SOW if clean.

## Delegation Plan

Implementer:

- Local implementation by the project manager, per current user routing.

Reviewers:

- No early external reviewer run for this small measurement option. If this
  becomes a broader production option, review as part of the larger reader
  optimization batch.

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

- If live/whole-file mmap is unsafe or slower, record the evidence and leave the
  option non-default or remove it before closing.

## Execution Log

### 2026-05-29

- Created SOW after the user requested adding the Rust option needed for
  measurement.
- Implemented live whole-file mmap as an explicit Rust reader option:
  - `WindowManager::new_with_strategy()` now accepts
    `ExperimentalMmapStrategy` for live readers while `WindowManager::new()`
    remains live/windowed.
  - `JournalFile::open_path_with_strategy()` and `open_with_strategy()` route
    live file opens through the selected strategy.
  - `ReaderBounds::Live` in the public Rust reader now honors
    `ReaderOptions::mmap_strategy`; defaults remain live/windowed.
  - `reader_core_bench` core mode now honors live `--mmap-strategy`.
- Added Rust live/whole-file reader-core benchmark cases for single-file
  `core-payloads`, `sdk-entry`, `sdk-payloads`, `facade-data`, and open-files
  `sdk-entry`, `sdk-payloads`, `facade-data`.
- Added `live_whole_file_maps_cached_file_once_and_remaps_on_growth` to prove
  live whole-file mmap maps the cached file once, does not refresh on in-range
  reads after append, and remaps only when a later read reaches appended bytes.
- Ran Rust-only compact/offline reader benchmark:
  - Command: `python3 tests/benchmarks/run_reader_core_benchmarks.py --languages rust --rows 100000 --directory-rows 100000 --repetitions 5 --warmups 1 --format compact --final-state offline --max-size-bytes 134217728 --directory-max-size-bytes 134217728 --window-size 33554432 --out .local/benchmarks/reader-core-rust-live-whole-file`
  - Result directory:
    `.local/benchmarks/reader-core-rust-live-whole-file/20260529T132501Z`
  - Result: live whole-file mmap is roughly equal to live/windowed on the main
    single-file payload path and does not explain the Go/Rust reader gap:

| Surface | Mode | Live/windowed rows/s | Live/whole-file rows/s | Ratio |
| --- | --- | ---: | ---: | ---: |
| file | core-payloads | 1,298,361 | 1,315,555 | 1.013 |
| file | sdk-entry | 117,809 | 115,669 | 0.982 |
| file | sdk-payloads | 2,515,113 | 2,518,273 | 1.001 |
| file | facade-data | 2,218,438 | 2,252,779 | 1.015 |
| open-files | sdk-entry | 113,316 | 103,859 | 0.917 |
| open-files | sdk-payloads | 1,985,906 | 2,046,748 | 1.031 |
| open-files | facade-data | 2,025,778 | 1,958,378 | 0.967 |

## Validation

Acceptance criteria evidence:

- Rust live readers can opt into whole-file mmap through
  `ReaderOptions::with_mmap_strategy(ExperimentalMmapStrategy::WholeFile)`:
  `rust/src/journal/src/lib.rs` now routes `ReaderBounds::Live` to
  `JournalFile::open_path_with_strategy()`.
- Default Rust live readers remain windowed: `ReaderOptions::default()` keeps
  `ExperimentalMmapStrategy::Windowed`, and `WindowManager::new()` still calls
  `new_with_strategy(..., Windowed)`.
- Reader-core benchmarks include Rust live/whole-file cases in
  `tests/benchmarks/run_reader_core_benchmarks.py`.
- Targeted mmap coverage added in
  `rust/src/crates/journal-core/src/file/mmap.rs`.
- Benchmark evidence recorded in the execution log above.

Tests or equivalent validation:

- `cargo fmt --manifest-path rust/Cargo.toml --all`
- `cargo test --manifest-path rust/Cargo.toml -p journal-core live_whole_file_maps_cached_file_once_and_remaps_on_growth`
- `cargo test --manifest-path rust/Cargo.toml -p journal-core`
- `cargo test --manifest-path rust/Cargo.toml -p journal`
- `cargo build --manifest-path rust/Cargo.toml -p reader_core_bench --release`
- `python3 tests/benchmarks/run_reader_core_benchmarks.py --languages rust --rows 100000 --directory-rows 100000 --repetitions 5 --warmups 1 --format compact --final-state offline --max-size-bytes 134217728 --directory-max-size-bytes 134217728 --window-size 33554432 --out .local/benchmarks/reader-core-rust-live-whole-file`
- `git diff --check`
- `.agents/sow/audit.sh`

Real-use evidence:

- The benchmark used repository-generated compact/offline journal fixtures and
  the Rust SDK reader binary, exercising the same file-backed reader paths used
  by the SDK benchmark suite.

Reviewer findings:

- Minimax: PRODUCTION GRADE for this SOW. It found no blocking issues and
  verified default preservation, routing correctness, benchmark validity, and
  adequate test coverage. It raised one low advisory that
  `get_whole_file_window()` clears old mappings before creating a new one; the
  borrowed window reference is consumed immediately by `get_slice()`, so no
  code change was required.
- Kimi: PRODUCTION GRADE for this SOW. It raised one medium observation that
  the benchmark is static/offline rather than true live append. Disposition:
  accepted and already scoped in the SOW/spec as compact/offline measurement
  evidence only; production live-append evaluation remains part of the broader
  performance program if this option graduates from experimental.
- Kimi also raised low observations about whole-file live mmap increasing
  SIGBUS blast radius on truncation and the absence of a full-stack
  `FileReader` unit test. Disposition: accepted as non-blocking because the
  option is experimental, defaults are unchanged, the benchmark exercises the
  full SDK path, and truncation risk is documented as virtual-memory/liveness
  tradeoff rather than a production recommendation.
- Qwen: PRODUCTION GRADE for this SOW. It found no blocking, high, or critical
  issues; it independently verified the SOW table against `summary.json`,
  default preservation, benchmark conclusion, and unchanged compatibility
  paths.

Same-failure scan:

- Searched for `open_path_with_strategy`, `new_with_strategy`, and
  `mmap_strategy` call sites. The benchmark core path, public SDK reader path,
  and default windowed live path are covered by the code changes.

Sensitive data gate:

- Durable artifacts contain generated benchmark paths, code paths, and
  aggregate throughput numbers only. No raw secrets, credentials, bearer
  tokens, SNMP communities, customer data, personal data, private endpoints, or
  production logs were recorded.

Artifact maintenance gate:

- AGENTS.md: no update needed; project workflow and guardrails did not change.
- Runtime project skills: no update needed; compatibility workflow did not
  change.
- Specs: `.agents/sow/specs/product-scope.md` updated to record live
  whole-file mmap as an explicit experimental measurement/performance option.
- End-user/operator docs: no README update needed in this SOW because the
  option is still an experimental measurement knob and not a recommended public
  production mode.
- End-user/operator skills: no output/reference skill affected.
- SOW lifecycle: marked `Status: completed` and moved to `.agents/sow/done/`
  together with the implementation before commit.
- SOW-status.md: `.agents/sow/SOW-status.md` and root `SOW-status.md` updated
  when this SOW opened and again when it closed.

Specs update:

- `.agents/sow/specs/product-scope.md` updated.

Project skills update:

- No project skill update needed; this did not change how agents should work in
  the repository.

End-user/operator docs update:

- No end-user/operator docs update needed; the option remains experimental and
  is being used to answer the performance question.

End-user/operator skills update:

- No output/reference skills are affected.

Lessons:

- Live whole-file mmap is not the missing Rust reader optimization on the
  current compact/offline payload corpus; the Go/Rust gap should be searched in
  other differences, such as DATA header parsing, guard/lifetime overhead, or
  benchmark path shape.

Follow-up mapping:

- Further Rust reader optimization remains under the performance program
  tracked by SOW-0009; no new follow-up SOW is required yet.

## Outcome

Completed. Rust live readers now honor the existing experimental mmap strategy
option, defaults remain live/windowed, and reader-core benchmark evidence shows
live whole-file mmap does not explain the Go/Rust reader gap on the current
compact/offline payload corpus.

## Lessons Extracted

- Keep this option experimental unless a true live-append benchmark proves it
  is valuable enough to justify higher virtual-memory pressure.
- The next Rust reader optimization search should focus on non-mmap-strategy
  differences, especially DATA header parsing, guard/lifetime overhead, and
  benchmark path shape.

## Followup

None yet.

## Regression Log

None yet.
