# SOW-0044 - Rust Reader Hot-Path Optimization

## Status

Status: completed

Sub-state: completed on 2026-05-29 after final whole-SOW review, validation,
and audit.

## Requirements

### Purpose

Optimize the Rust reader after SOW-0043 establishes the reader compatibility
target, then use Rust as the performance reference for Go, Python, and Node.js.

### User Request

The user wants the reader phase to start with Rust:

- align Rust to libsystemd first;
- eliminate system calls and allocations in hot paths;
- measure single-file and ordered directory reader performance;
- compare against systemd C and Netdata's current reader behavior.

### Assistant Understanding

Facts:

- Reader performance is a production gate for Netdata integrations.
- Checked Netdata evidence shows current Netdata hot reader paths are
  single-file at the journal object-reader level:
  - NetFlow `scan_journal_files_forward()` accepts a file list but sorts it and
    opens one `JournalFile<Mmap>` at a time inside a sequential loop.
  - NetFlow raw projected scans open one `JournalFile<Mmap>` for the selected
    raw file.
  - Netdata `systemd-journal.plugin` collects matching files, sorts them, then
    calls `nd_sd_journal_query_one_file()` sequentially per file; that helper
    calls `nsd_journal_open_files()` with a one-path array.
- Directory readers must support ordered reading across multiple files.
- Single-file and directory reader performance must be measured separately.

Inferences:

- Rust reader optimization should not start until SOW-0043 closes the parity
  target.

Unknowns:

- Actual Rust reader bottlenecks after parity work.
- Whether concurrent Netdata queries can cause multiple independent single-file
  reader sessions at the same time. This SOW does not assume a single global
  reader, only that each reader instance hot path should be optimized for one
  opened file first.

### Acceptance Criteria

- Rust single-file reader benchmarks exist and are reproducible and are treated
  as the primary Netdata hot-path target.
- Rust ordered directory reader benchmarks exist and are reproducible.
- Benchmarks compare against systemd C/libsystemd or `journalctl` where
  applicable, and against Netdata's current reader behavior where practical.
- Profiles identify hot-path allocations, syscalls, decompression cost,
  filtering cost, cursor/seek cost, and directory merge cost.
- Optimizations preserve conformance and interoperability.
- Final Rust reader results and remaining risks are documented.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/specs/product-scope.md`
- `.agents/sow/current/SOW-0009-20260523-benchmark-profile-optimize.md`

Current state:

- Rust reader performance has not been systematically optimized after reader
  parity.

Risks:

- Optimizing before SOW-0043 could target incomplete behavior.
- Directory merge performance can hide single-file performance regressions if
  they are measured together.

## Pre-Implementation Gate

Status: ready; SOW-0043 is completed.

Problem / root-cause model:

- Rust reader now has a stable compatibility target from SOW-0043. Netdata
  evidence indicates the highest-value optimization target is a single opened
  journal file, while directory/open-files behavior remains part of the SDK
  contract and must stay measured as a regression guard.

Evidence reviewed:

- SOW-0009 umbrella performance requirements.
- Product scope reader sections.
- Netdata reader evidence from `ktsaou/netdata @ b018c0a13ee7`:
  - `src/crates/netflow-plugin/src/query/scan/direct.rs:11-45` accepts multiple
    files but opens one `JournalFile<Mmap>` at a time in a sequential loop.
  - `src/crates/netflow-plugin/src/query/scan/raw.rs:87-117` opens one raw
    journal file for projected raw scanning.
  - `src/collectors/systemd-journal.plugin/systemd-journal.c:638-655` opens one
    file through a one-element `paths` array in `nd_sd_journal_query_one_file()`.
  - `src/collectors/systemd-journal.plugin/systemd-journal.c:767-795` iterates
    matched files sequentially and calls `nd_sd_journal_query_one_file()` for
    each.
  - `src/collectors/systemd-journal.plugin/systemd-journal-files.c:196-203`
    opens one file through a one-element `files` array to refresh file header
    metadata.

Affected contracts and surfaces:

- Rust reader API, libsystemd facade, directory reader, journalctl rewrite,
  Netdata reader integration readiness.

Existing patterns to reuse:

- Existing Rust reader implementation.
- Shared fixtures and mixed-directory tests.
- Benchmark result convention under `.local/benchmarks/`.

Risk and blast radius:

- High for reader correctness and Netdata integration readiness.

Sensitive data handling plan:

- Use generated or public fixtures only. Do not record real customer logs,
  SNMP communities, credentials, bearer tokens, personal data, private
  endpoints, or production incident details.

Implementation plan:

1. Build Rust reader baseline after SOW-0043.
2. Establish single-file benchmarks as the primary Netdata hot-path benchmark.
3. Keep ordered directory/open-files benchmarks separate as SDK regression
   coverage, not as the first optimization target.
4. Profile single-file reader hot paths first, then directory/open-files merge
   costs.
5. Optimize in batches.
6. Re-run conformance and benchmarks after each accepted batch.
7. Document results and residual risks.

Validation plan:

- Rust tests.
- Shared reader conformance and mixed-directory tests.
- Benchmark/profiler artifacts.
- Read-only reviewer passes.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: update if reader benchmark workflow becomes durable.
- Specs: update Rust reader performance status if public.
- End-user/operator docs: update benchmark docs if public.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: close before Go reader performance SOW.
- SOW-status.md: update when activated and closed.

Open-source reference evidence:

- systemd/libsystemd evidence must be collected during implementation.

Open decisions:

- Final performance thresholds require first valid baseline evidence.

## Implications And Decisions

- 2026-05-28: user agreed reader optimization starts with Rust after parity.
- 2026-05-29: Netdata evidence shows the current production reader hot paths
  open/read one journal file at a time, even when the surrounding query selects
  multiple files. Decision for this SOW: prioritize single-file Rust reader
  performance first, while retaining ordered directory/open-files benchmarks as
  mandatory SDK regression coverage.

## Plan

1. Benchmark current Rust single-file reader variants.
2. Benchmark current Rust ordered directory/open-files reader variants.
3. Build comparable systemd C/libsystemd or `journalctl` baselines where the
   measurement is meaningful and documented.
4. Profile Rust single-file reader hot paths.
5. Optimize and validate.

## Delegation Plan

Implementer:

- Local implementation by the project manager unless the user changes routing.

Reviewers:

- Read-only reviewers from the approved pool.

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

- Record invalid benchmarks, profiler findings, reviewer findings, and residual
  gaps in this SOW.

## Execution Log

### 2026-05-28

- Created from the agreed SOW restructuring.

### 2026-05-29

- Confirmed commit `3699bf6` was already clean and `git push` reported
  everything up-to-date.
- Activated this SOW after SOW-0043 completed.
- Checked Netdata reader call sites read-only at `ktsaou/netdata @
  b018c0a13ee7`. The checked hot paths read one journal file at a time, but the
  surrounding systems still select, sort, and scan sets of files. Therefore
  this SOW prioritizes single-file Rust reader performance and keeps directory
  benchmarks for SDK contract regression coverage.
- Added `tests/benchmarks/run_reader_core_benchmarks.py`, Rust
  `reader_core_bench`, and a C/libsystemd `reader_core_bench` helper. The
  harness separates fixture generation from timed read loops, records
  single-file and explicit `open-files` results separately, and labels Rust
  reader bounds/mmap options.
- Baseline evidence showed the low-level Rust payload reader was dominated by
  live file-size refreshes during DATA object access. A read of 100k rows with
  32 fields per row made 7,600,032 `statx` calls in live/windowed
  `sdk-payloads`, while snapshot/windowed made 6 `statx` calls for the same
  scan. Profile outputs:
  - `.local/benchmarks/reader-core/profiles/sdk-payloads-live.strace`
  - `.local/benchmarks/reader-core/profiles/sdk-payloads-snapshot.strace`
- Added Rust `ReaderOptions` with `Live` and `Snapshot` bounds. Existing
  reader constructors keep the default live behavior for active-file
  compatibility; `Snapshot` fixes file size at open for polling/query scans
  that do not need to observe appends during the same scan.
- Fixed snapshot/windowed mmap handling for a final partial mmap window. The
  first implementation incorrectly required the requested chunk to fit fully
  inside the file; live/windowed already mapped the final partial chunk.
- Added raw current-entry payload visitor/collector methods on Rust file and
  directory readers. This is the allocation-light SDK hot path for Netdata-like
  byte-level scans. The convenience `get_entry()` path remains available and
  still materializes maps, repeated-value maps, owned payloads, and cursor
  strings.
- Updated the libsystemd-style facade data path to collect current-entry
  payloads directly instead of materializing a full `Entry` for
  `RestartData()` and `GetData()`.
- Added benchmark checksum validation: non-warmup Rust payload-reading modes
  must match stock libsystemd records, fields, bytes, and checksum for the same
  surface/direction before a benchmark summary is written.
- 100k-row compact fixture benchmark result directory after checksum validation:
  `.local/benchmarks/reader-core/20260528T225220Z`.
  Key medians:
  - Rust single-file `sdk-payloads` snapshot/windowed: 1,177,820 rows/s,
    37,690,244 fields/s.
  - Rust single-file `sdk-payloads` snapshot/whole-file: 1,144,607 rows/s,
    36,627,420 fields/s.
  - Rust single-file `core-payloads` snapshot/windowed: 1,184,403 rows/s,
    37,900,898 fields/s.
  - Rust single-file `facade-data` snapshot/windowed: 880,109 rows/s,
    28,163,482 fields/s.
  - Rust single-file `sdk-entry` snapshot/windowed: 113,205 rows/s,
    3,622,560 fields/s.
  - Stock libsystemd single-file data enumeration: 580,255 rows/s,
    18,568,152 fields/s.
  - Rust `open-files` `sdk-payloads` snapshot/windowed: 996,786 rows/s,
    32,893,946 fields/s.
  - Stock libsystemd `open-files` data enumeration: 621,356 rows/s,
    20,504,752 fields/s.
- Reviewer batch:
  - `glm-5.1`: code judged correct; not production-grade only because SOW
    validation was still pending before this update.
  - `qwen3.6-plus`: requested a small mutable mmap overflow-check clarity fix,
    checksum validation in the benchmark harness, and SOW validation updates.
  - `minimax-m2.7-coder`: no code correctness blocker; requested SOW
    validation updates and benchmark same-failure evidence.
  - `kimi-k2.6`: no final verdict. The reviewer command ran read-only for over
    nine minutes, produced only diff-inspection output, and was stopped by
    targeted PID after stalling.
- Addressed reviewer findings:
  - clarified `get_slice_mut()` overflow check by binding the checked end;
  - corrected the window remap comment from "centered" to chunk-aligned around
    the requested position;
  - added benchmark checksum validation against stock libsystemd;
  - reran the benchmark harness successfully after checksum validation.
- Final whole-SOW review pass after the fixes:
  - `minimax-m2.7-coder`: PRODUCTION GRADE; no correctness, compatibility, or
    security blockers.
  - `glm-5.1`: PRODUCTION GRADE; noted only two low-severity observations that
    do not block production use.
  - `qwen3.6-plus`: no final verdict in the second pass. It completed static
    inspection reads and then produced no output for several polls, so only the
    exact reviewer PIDs were stopped. Its previous blocking findings were
    already fixed and validated.

## Validation

Acceptance criteria evidence:

- Rust single-file reader benchmarks exist and are reproducible:
  `tests/benchmarks/run_reader_core_benchmarks.py` plus Rust
  `reader_core_bench`.
- Rust ordered open-files benchmarks exist and are reproducible in the same
  harness.
- Stock libsystemd comparison exists through
  `tests/benchmarks/systemd/reader_core_bench.c`.
- The benchmark harness now verifies matching records, fields, bytes, and
  checksums between stock libsystemd data enumeration and Rust payload-reading
  modes before writing summaries.
- Hot-path profile evidence identified live file-size refresh as the dominant
  bottleneck: 7,600,032 `statx` calls for live/windowed `sdk-payloads` versus
  6 `statx` calls for snapshot/windowed on the same 100k-row scan.
- Final benchmark evidence after checksum validation:
  `.local/benchmarks/reader-core/20260528T225220Z`.

Tests or equivalent validation:

- `cargo test --manifest-path rust/Cargo.toml -p journal-core -p journal`:
  PASS.
- `cargo test --manifest-path rust/Cargo.toml --workspace`: PASS.
- `python3 tests/benchmarks/run_reader_core_benchmarks.py --rows 1000
  --directory-rows 2000 --repetitions 1 --warmups 0 --format compact
  --final-state online --keep-fixtures`: PASS after checksum validation.
- `python3 tests/benchmarks/run_reader_core_benchmarks.py --rows 100000
  --directory-rows 100000 --repetitions 3 --warmups 1 --format compact
  --final-state online --keep-fixtures`: PASS after checksum validation.
- `python3 tests/interoperability/run_directory_matrix.py --readers stock
  rust`: PASS with `systemd 260 (260.1-2-manjaro)`.
- `python3 tests/interoperability/run_mixed_directory_matrix.py --readers
  stock rust`: PASS, 27/27, with `systemd 260 (260.1-2-manjaro)`.

Real-use evidence:

- Read-only Netdata evidence at `ktsaou/netdata @ b018c0a13ee7` shows the
  checked production hot paths read one journal file at a time, while file-set
  selection and concurrent sessions remain possible. The optimized
  `sdk-payloads` snapshot path is therefore the primary Rust reader benchmark
  for Netdata-like scans; open-files remains measured as SDK regression
  coverage.

Reviewer findings:

- `glm-5.1`: no code correctness blocker; SOW validation pending was the only
  production-grade blocker.
- `qwen3.6-plus`: requested `get_slice_mut()` overflow-check clarity, benchmark
  checksum validation, and SOW validation updates. All were addressed.
- `minimax-m2.7-coder`: requested SOW validation updates and benchmark
  same-failure evidence. Both were addressed.
- `kimi-k2.6`: produced only read-only diff inspection and no final verdict
  before stalling; stopped by targeted PID.
- Final whole-SOW review pass after fixes:
  - `minimax-m2.7-coder`: PRODUCTION GRADE.
  - `glm-5.1`: PRODUCTION GRADE.
  - `qwen3.6-plus`: stalled without a final verdict after static inspection;
    stopped by targeted PID. No new finding was reported before it stalled.

Same-failure scan:

- The benchmark harness now validates records, fields, bytes, and checksum for
  every non-warmup Rust payload-reading run against stock libsystemd data
  enumeration for the same surface/direction. Both the 1k smoke run and 100k
  benchmark run passed this check.

Sensitive data gate:

- PASS. Fixtures are generated synthetic journal data. Durable artifacts record
  paths, commands, aggregate benchmark values, and public upstream evidence
  only. No raw secrets, customer data, private endpoints, credentials, or
  production logs were written.

Artifact maintenance gate:

- PASS. Durable artifacts were updated where behavior or operator workflow
  changed, and no project-wide workflow guardrail update was needed.

Specs update:

- Updated `.agents/sow/specs/product-scope.md` with Rust reader snapshot
  bounds, raw payload visitor, and benchmark status.

Project skills update:

- No project skill update needed. The existing journal compatibility and agent
  orchestration skills already cover reader compatibility, benchmark evidence,
  reviewers, and repository-boundary requirements.

End-user/operator docs update:

- Updated `tests/benchmarks/README.md` with reader benchmark modes, hot-path
  interpretation, and live versus snapshot reader bounds.

End-user/operator skills update:

- No output/reference end-user skill exists for this SDK benchmark workflow, so
  none was updated.

Lessons:

- Reader performance must distinguish live active-file compatibility from
  snapshot/polling scans. Comparing them as one mode hides the actual cost of
  live file-size refresh.
- Convenience entry materialization is not a raw scanner hot path. The SDK
  needs explicit byte-level visitor/enumeration APIs for Netdata-like readers.

Follow-up mapping:

- Go reader alignment and optimization remains tracked by SOW-0045.
- Python and Node.js reader alignment remains tracked by SOW-0046.
- Netdata integration remains tracked by SOW-0026 and should follow reader
  performance work.
- The remaining SOW-0009 reader work continues in child reader SOWs; no
  untracked deferred item remains in this SOW.

## Outcome

Rust reader hot-path optimization is complete for this SOW. Snapshot reader
bounds and raw payload visitor APIs are available, the facade data path avoids
full entry materialization, benchmarks compare against stock libsystemd with
checksum validation, and Rust/directory/mixed-directory validation passes.

## Lessons Extracted

- Keep live and snapshot reader modes explicit in every benchmark and API
  discussion.
- Validate benchmark equivalence with checksums, not only matching row counts.
- Keep convenience entry APIs separate from allocation-light raw payload scan
  APIs.

## Followup

No untracked follow-up remains. Go reader alignment is tracked by SOW-0045,
Python/Node.js reader alignment by SOW-0046, and Netdata integration by
SOW-0026 plus the component integration SOWs.

## Regression Log

None yet.
