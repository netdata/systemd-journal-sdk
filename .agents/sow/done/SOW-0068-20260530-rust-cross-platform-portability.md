# SOW-0068 - Rust Cross Platform Portability

## Status

Status: completed

Sub-state: completed; implementation, reviewer gates, native macOS/Windows
validation, and orchestrator lifecycle reconciliation finished.

## Requirements

### Purpose

Make the Rust SDK reader, writer, directory writer, and file-backed journalctl
paths portable to Linux, FreeBSD, macOS, and Windows while keeping Rust as the
reference implementation for compatibility and performance.

### User Request

The user requested SDK portability to Linux, FreeBSD, macOS, and Windows, and
approved parallel worktrees for independent SOWs.

### Assistant Understanding

Facts:

- This SOW covers Rust only.
- SOW-0063 recorded that Rust Windows target checking failed in
  `journal-common` because `nix` time APIs were unavailable on the checked
  Windows target.
- SOW-0063 recorded Linux `/proc` assumptions in Rust stale-lock owner
  detection.
- Rust is the reference implementation for cross-language behavior.

Inferences:

- Rust needs platform modules for time, locking, process identity, boot ID,
  directory sync, and mmap/fallback behavior.
- Linux behavior and reader row-scoped lifetime guarantees must not weaken.

Unknowns:

- Which non-Linux runtime environments are available locally for execution.

### Acceptance Criteria

- Linux Rust tests pass for affected crates.
- Windows Rust target checks compile for the SDK crates in scope, or exact
  target/toolchain blockers are recorded.
- FreeBSD and macOS target checks are added or blockers are recorded.
- Rust writer locking preserves one-writer behavior on supported targets.
- Rust reader/writer behavior keeps existing shared conformance and
  interoperability results on Linux.
- Linux Rust performance and facade lifetime guarantees do not regress.
- Specs/docs describe Rust platform behavior.

## Analysis

Sources checked:

- `rust/src/crates/journal-core/src/file/lock.rs`
- `rust/src/crates/journal-core/src/file/file.rs`
- `rust/src/crates/journal-common/src/time.rs`
- `.agents/sow/pending/SOW-0063-20260530-cross-platform-portability.md`
- `.agents/skills/project-journal-compatibility/SKILL.md`

Current state:

- Rust lock stale-owner detection reads Linux `/proc`.
- Rust Windows target check failed in `journal-common` time code.
- Linux behavior is currently the compatibility and performance reference.

Risks:

- Refactoring platform code can accidentally change Linux semantics.
- Rust row-scoped reader data lifetime guarantees must remain intact.
- Windows locking and file sharing require explicit behavior.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Rust portability is blocked by Linux/POSIX assumptions in common time and
  locking helpers. Format-level code is mostly OS-independent, but platform
  integration is not isolated.

Evidence reviewed:

- SOW-0063 target-check and source evidence.
- Project compatibility skill requirements for compatibility, live safety, and
  row-scoped facade data.

Affected contracts and surfaces:

- Rust crates under `rust/src/crates/`.
- Rust SDK reader/writer APIs and libsystemd facade.
- Rust journalctl rewrite.
- Locking, directory sync, mmap/fallback, boot/process identity, and time APIs.

Existing patterns to reuse:

- Rust `cfg` modules.
- Existing lockfile format `systemd-journal-sdk-lock-v1`.
- Existing row-scoped facade lifetime model.
- Existing interoperability and benchmark runners.

Risk and blast radius:

- High because Rust is the reference implementation.

Sensitive data handling plan:

- Use synthetic fixtures only; do not read host live journals or record raw log
  payloads.

Implementation plan:

1. Isolate Rust platform assumptions behind target-specific modules.
2. Preserve Linux code paths and hot behavior.
3. Add target checks and tests.
4. Update docs/specs and SOW validation.

Validation plan:

- Linux cargo tests for affected crates.
- Windows target check with cache/output paths under `.local/`.
- FreeBSD/macOS checks or exact blocker evidence.
- Relevant interoperability and lock tests on Linux.
- Benchmark smoke check if hot paths change.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: no update expected.
- Specs: update cross-platform behavior.
- End-user/operator docs: update Rust docs.
- End-user/operator skills: no update expected.
- SOW lifecycle: child of SOW-0063.
- SOW-status.md: reconciliation left to orchestrator per worktree prompt.

Open-source reference evidence:

- None added; baseline remains systemd/systemd v260.1 from project specs.

Open decisions:

- None. User approved parallel worktree execution.

## Implications And Decisions

1. 2026-05-30: This SOW is assigned to an isolated worktree. It should not edit
   other language implementations except shared specs/docs/tests required by the
   Rust portability contract.

## Plan

1. Isolate Rust platform code.
2. Implement portable lock, time, and identity helpers.
3. Validate and document.

## Delegation Plan

Implementer:

- User-spawned implementation agent in a dedicated worktree.

Reviewers:

- Whole-SOW read-only reviewer pass after implementation and local validation.

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

- Append questions or blockers to this SOW under `## Agent Questions -
  YYYY-MM-DD` with evidence, options, and a recommendation, then stop.

## Execution Log

### 2026-05-30

- Created as Rust-only child SOW under SOW-0063 for parallel worktree
  execution.
- Confirmed user-authorized parallel implementation routing; AGENTS.md
  external-implementer exception applies for this worktree.
- Moved from `.agents/sow/pending/` to `.agents/sow/current/` and changed
  `Status: open` to `Status: in-progress`.
- Replaced `journal-common` unconditional `nix` time usage with platform
  clock helpers: Unix uses `clock_gettime(CLOCK_MONOTONIC)` through `libc`;
  Windows uses `QueryUnbiasedInterruptTime`; other non-Unix targets use a
  process-local `Instant` fallback.
- Replaced Rust writer lock Linux `/proc` assumptions with platform helpers:
  Linux keeps `/proc/<pid>/stat` start-time verification, FreeBSD/macOS use
  PID liveness, and Windows uses process creation time plus
  `OpenProcess`/`WaitForSingleObject` status.
- Added FreeBSD identity support for common `machine-id` paths and FreeBSD
  boot ID derivation from `sysctl kern.boottime`.
- Kept Linux mmap hot paths intact and added a non-Unix fallback for header
  rewrite during sync.
- Added non-Unix no-op SIGBUS and directory-sync hooks where Unix-only
  mechanisms do not exist.
- Updated `rust/README.md` and `.agents/sow/specs/product-scope.md` with Rust
  platform behavior and non-Linux validation limits.
- Ran whole-SOW read-only reviewer round 1 with
  `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/qwen3.6-plus`,
  `llm-netdata-cloud/glm-5.1`, `llm-netdata-cloud/minimax-m2.7-coder`, and
  `llm-netdata-cloud/mimo-v2.5-pro`.
- Implemented real reviewer findings: restored Linux `/host/proc` boot-ID
  fallback, replaced Windows process-local monotonic timestamps with Windows
  unbiased interrupt time, added Windows process creation-time stale-lock
  identity checks, treated BSD/macOS invalid PIDs as stale, target-gated
  `journal-common` `libc`, simplified macOS UUID parsing, added a common
  FreeBSD machine-id path, and corrected docs/spec overclaims.
- Rejected reviewer-suggested removal of `OpenOptionsExt` because Linux tests
  proved `journal-core/src/file/file.rs` still uses `.mode(0o640)`.
- Reran the whole-SOW read-only reviewer scope after the fix commit. Captured
  final votes from all five approved reviewers:
  `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/qwen3.6-plus`,
  `llm-netdata-cloud/glm-5.1`, `llm-netdata-cloud/minimax-m2.7-coder`, and
  `llm-netdata-cloud/mimo-v2.5-pro` all voted `PRODUCTION GRADE`.

## Validation

Acceptance criteria evidence:

- Linux Rust tests passed for affected crates:
  `journal-common`, `journal-core`, `journal-log-writer`, `journal`, and
  `journalctl`.
- Windows Rust target checks passed for SDK crates in scope:
  `journal-common`, `journal-core`, `journal-log-writer`, `journal`, and
  `journalctl`, including `--tests`.
- FreeBSD/macOS target checks were attempted and blocked before crate code by
  missing installed Rust standard-library targets:
  `x86_64-unknown-freebsd` and `x86_64-apple-darwin`.
- Rust writer locking was validated on Linux with the affected unit tests plus
  a Rust-only livewriter contention and stale-lock recovery probe.
- Existing facade row-scoped lifetime tests remained passing in the public
  `journal` crate test set.
- Linux writer smoke check produced a stock-verifiable regular journal file.
- Specs/docs now describe Rust platform behavior.

Tests or equivalent validation:

- Succeeded:
  `CARGO_HOME="$PWD/.local/cargo-home" CARGO_TARGET_DIR="$PWD/.local/cargo-target" cargo check --manifest-path rust/Cargo.toml -p journal-common -p journal-core -p journal-log-writer -p journal -p journalctl --tests --target x86_64-pc-windows-gnu`
- Succeeded:
  `CARGO_HOME="$PWD/.local/cargo-home" CARGO_TARGET_DIR="$PWD/.local/cargo-target" cargo test --manifest-path rust/Cargo.toml -p journal-common -p journal-core -p journal-log-writer -p journal -p journalctl`
  Result summary: 22 `journal` tests, 31 `journal-common` tests, 63
  `journal-core` tests, 2 `journal-log-writer` unit tests, 48
  `journal-log-writer` integration tests, 9 `journalctl` tests, and applicable
  doctests passed.
- Succeeded:
  Rust-only livewriter lock contention and stale-lock recovery probe using
  `.local/interoperability/bin/rust-livewriter`.
  Result artifact:
  `.local/interoperability/rust-lock-validation/result-20260530-091236.json`.
  Contention, stale-lock recovery, lock cleanup, and stock verification all
  succeeded.
- Succeeded:
  writer smoke:
  `CARGO_HOME="$PWD/.local/cargo-home" CARGO_TARGET_DIR="$PWD/.local/cargo-target" cargo run --manifest-path rust/Cargo.toml -p writer_core_bench -- --output "$PWD/.local/review-smoke/fedcba9876543210fedcba9876543210/system.journal" --rows 1000 --format regular --surface direct --api-mode raw-payload --final-state offline`
  Result: 1000 rows, 32 fields per row, regular direct writer, offline close,
  `append_rows_per_second` about `41283`, `errors: []`.
- Succeeded:
  `journalctl --verify --file "$PWD/.local/review-smoke/fedcba9876543210fedcba9876543210/system.journal"`
- Succeeded: `git diff --check`.
- Succeeded: `.agents/sow/audit.sh`.
- BLOCKED by local toolchain:
  `cargo check --manifest-path rust/Cargo.toml -p journal-core --target x86_64-apple-darwin`
  failed with `can't find crate for core/std`; rustc reported the target may
  not be installed and suggested `rustup target add x86_64-apple-darwin`.
- BLOCKED by local toolchain:
  `cargo check --manifest-path rust/Cargo.toml -p journal-core --target x86_64-unknown-freebsd`
  failed with `can't find crate for core/std`; rustc reported the target may
  not be installed and suggested `rustup target add x86_64-unknown-freebsd`.
- BLOCKED outside Rust SOW scope:
  full Windows workspace check reached `journal-engine` dependency
  `foyer -> foyer-storage -> lz4 -> lz4-sys`, which failed because
  `x86_64-w64-mingw32-gcc` is not installed.
- BLOCKED outside Rust-only validation scope:
  `tests/interoperability/run_lock_matrix.py --entries 40 --delay-ms 5`
  failed before Rust holder validation because the Node.js writer path could
  not load `node/src/lib/lz4-block.js` dependency `lz4`.

Real-use evidence:

- Stock `journalctl --verify --file` passed for the Rust benchmark smoke
  journal written after the portability changes.
- Existing stock-verify Rust tests passed, including compact writer/reader,
  sealed writer, tampered sealed data, wrong-key sealed verification, and
  public verification API fixture tests.
- Rust-only livewriter validation proved that one active Rust writer rejects a
  second Rust writer before ready publication and that a crashed Rust writer's
  stale lock can be reclaimed by the next Rust writer.

Reviewer findings:

- Round 1, `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`.
  Findings: invalid-PID `EINVAL` on non-Linux Unix could leave a stale lock
  unreclaimed; hardcoded mmap page-size is a pre-existing runtime portability
  risk; Windows integration tests would panic if executed where machine IDs
  are unsupported; `OpenOptionsExt` looked unused; SIGBUS mmap return handling
  is pre-existing. Disposition: implemented `EINVAL` stale handling; rejected
  `OpenOptionsExt` removal because `.mode(0o640)` requires it; tracked page
  size, Windows runtime-test fallback, and SIGBUS return handling as
  non-blocking/pre-existing because current SOW validates non-Linux
  compilation, not runtime certification.
- Round 1, `llm-netdata-cloud/qwen3.6-plus`: `NOT PRODUCTION GRADE`.
  Findings: Linux `load_boot_id()` lost `/host/proc` fallback; Windows
  monotonic timestamps were process-relative; `journal-common` pulled `libc`
  into Windows builds; FreeBSD `/host/` fallback and synthetic boot IDs needed
  clearer documentation. Disposition: fixed Linux boot-ID host fallback,
  replaced Windows monotonic clock with `QueryUnbiasedInterruptTime`, moved
  `libc` under `cfg(unix)`, and clarified synthetic boot-ID docs.
- Round 1, `llm-netdata-cloud/glm-5.1`: `NOT PRODUCTION GRADE`.
  Findings: Windows boot IDs degrade to empty strings; macOS UUID parsing was
  more fragile than needed; `monotonic_now()` documentation overclaimed
  universal `CLOCK_MONOTONIC`; synthetic boot IDs needed clearer comments.
  Disposition: added Windows process creation-time identity checks so stale
  locks do not depend only on empty boot IDs and PID liveness; simplified
  macOS UUID parsing; corrected `monotonic_now()` comments and synthetic
  boot-ID comments.
- Round 1, `llm-netdata-cloud/minimax-m2.7-coder`: `NOT PRODUCTION GRADE`.
  Findings: non-Linux Unix lock error handling and same-boot wording needed
  review; Windows process-local monotonic time was weaker than desired;
  FreeBSD boot-ID derivation was synthetic; mmap/header and directory-sync
  non-Unix fallbacks have lower durability/atomicity than Unix. Disposition:
  fixed invalid-PID stale handling, Windows monotonic source, and docs wording;
  left documented non-Unix sync/header trade-offs unchanged because they are
  part of this SOW's portable fallback surface and Linux hot paths remain
  intact.
- Round 1, `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`.
  Findings: Windows empty boot ID, process-local monotonic time, and no-op
  directory sync were non-blocking as documented; SIGBUS return-value handling
  is pre-existing. Disposition: still improved Windows monotonic and process
  identity based on other reviewers; left no-op directory sync and SIGBUS
  return-value handling as documented/pre-existing risks.
- Round 2, `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`.
  Findings: non-blocking risks only. Noted `journal-core` still has an
  unconditional `libc` dependency, Linux lock owner checks still treat all
  `/proc` errors as stale as in the pre-SOW behavior, macOS `system_profiler`
  is slow, FreeBSD/macOS `sysctl` text parsing is fragile, and 4096-byte page
  assumptions are pre-existing. Disposition: no new code changes; these are
  either pre-existing, documented, or cleanup outside the current validated
  scope.
- Round 2, `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`.
  Findings: non-blocking risks only. Noted unconditional `journal-core`
  `libc`, pre-existing 4096-byte page assumptions, no-op non-Unix directory
  sync, seek/write non-Unix header rewrite, synthetic FreeBSD/macOS boot IDs,
  and Linux `lock.rs` direct `/proc` boot-ID read instead of the generic
  `/host/` fallback. Disposition: no blocker; behavior is conservative or
  documented, and Linux runtime validation remains clean.
- Round 2, `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`.
  Findings: non-blocking risks only. Confirmed all round-1 fixes, Linux test
  suite, Windows target checks, dependency changes, and docs/spec claims.
  Noted the same low-risk cleanup candidates: `journal-core` `libc` gating,
  hardcoded page size, slow macOS identity lookup, synthetic boot IDs, and
  no-op non-Unix directory sync. Disposition: no code changes after final
  review.
- Round 2, `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE`.
  Findings: non-blocking risks only. Noted non-Linux Unix PID reuse can keep a
  stale lock held, Windows lock acquisition uses additional syscalls, non-Unix
  mmap/SIGBUS fallbacks have lower durability/atomicity, and Linux lock
  `boot_id()` does not use the generic `/host/proc` fallback. Disposition:
  accepted as documented conservative behavior in the SOW scope.
- Round 2, `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`.
  Findings: non-blocking risks only. Noted `journal-core` unconditional
  `libc`, non-Linux Unix PID reuse gap, no-op non-Unix directory sync,
  pre-existing hardcoded page size/SIGBUS return-value handling, and Windows
  empty boot ID compensated by creation-time/wait-status checks. Disposition:
  no blocker; the implementation is production-grade within the SOW's stated
  Linux-runtime/non-Linux-compilation scope.

Same-failure scan:

- `rg -n "\bnix::|nix =" rust/src/crates/journal-common rust/src/crates/journal-core rust/src/crates/journal-log-writer rust/src/journal rust/src/cmd/journalctl rust/Cargo.toml`
  returned no matches after removing `nix` from the affected crates.
- `rg -n "process-local monotonic|same-boot PID|External reviewers were not run|reviewer pass is left|left to the orchestrator|nix::|nix =" ...`
  found no stale reviewer-handoff text after this update; the only remaining
  `process-local monotonic` mention is the explicit fallback for unsupported
  non-Unix/non-Windows targets in `journal-common/src/time.rs`.
- `rg -n "/proc/sys/kernel/random/boot_id|/proc/\{pid\}/stat|std::os::unix::ffi::OsStrExt|write_all_at|MAP_ANONYMOUS" ...`
  found only target-gated Linux/Unix usages or Unix-only tests:
  `journal-core/src/file/lock.rs`, `journal-common/src/system.rs`,
  `journal-core/src/file/sigbus.rs`, and the Unix test tamper helper in
  `journal-core/src/file/writer.rs`.

Sensitive data gate:

- PASS. Work used synthetic fixtures and generated journals under `.local/`.
  No live host journal was probed, and `.agents/sow/audit.sh` sensitive-data
  scan reported clean durable artifacts.

Artifact maintenance gate:

- `AGENTS.md`: not updated; workflow rules did not change.
- Runtime project skills: not updated; no reusable project workflow changed.
- Specs: updated `.agents/sow/specs/product-scope.md` with Rust platform
  behavior.
- End-user/operator docs: updated `rust/README.md` with Rust platform behavior
  and validation limits.
- End-user/operator skills: none affected; this repository has no output skill
  for Rust SDK consumers.
- SOW lifecycle: reconciled by orchestrator as completed and moved to
  `.agents/sow/done/`.
- `SOW-status.md`: updated to list SOW-0068 as completed.

Lessons extracted:

- The Rust SDK crates can be made Windows-target clean without weakening Linux
  `/proc` start-time lock validation by isolating liveness checks per target.
- Full-workspace Windows target checking is currently limited by
  `journal-engine`'s native `lz4-sys` build chain, which is separate from the
  public Rust SDK crates validated here.

Follow-up mapping:

- Parent umbrella: `SOW-0063-20260530-cross-platform-portability.md`.
- Parent/CI follow-up: collect runtime evidence on FreeBSD, macOS, and Windows
  machines or runners, then validate generated files on Linux with stock
  systemd tooling.
- Separate portability follow-up if full Rust workspace portability is desired:
  decide whether `journal-engine` keeps `foyer`/native `lz4-sys` for
  non-public indexing/cache paths or receives a target-specific dependency
  configuration.
- Cleanup candidates for a future SOW if runtime portability hardening is
  expanded: target-gate `journal-core` `libc`, replace pre-existing 4096-byte
  page assumptions with runtime page-size detection, harden SIGBUS
  `mmap()`-failure handling, and decide whether Linux lock boot-ID lookup
  should share the generic `/host/` fallback.

## Outcome

Rust cross-platform portability work is complete for this child SOW. Linux
tests, Windows target checks, whole-SOW reviewer rounds, native macOS/Windows
validation under SOW-0063, and Linux stock verification of non-Linux generated
journal files passed. The native FreeBSD runtime gap and SOW-0071 runtime-purity
split stay with parent SOW-0063, not this Rust-only child SOW.

## Parent Native Validation Addendum - 2026-05-30

Facts:

- Parent SOW-0063 ran native Rust validation on approved macOS and Windows
  hosts after the child portability work.
- Native validation found additional Rust portability debt and repaired it in
  this branch.

Repairs:

- `journal-registry` now accepts native absolute paths in `File::from_path()`
  and `File::from_raw_path()`.
- non-Linux Unix stale-lock checks now reject invalid or wrapped PIDs before
  calling `kill(pid, 0)`.
- `journal-common` realtime-clock coverage no longer depends on the host clock
  advancing by exactly one microsecond.
- `journal-log-writer` tests use deterministic machine and boot IDs instead of
  host identity helpers.
- `journal-log-writer` `serde-api` no longer depends on the Git
  `flatten-serde-json` package that failed Windows checkout under repo-local
  Cargo caches.

Evidence:

- Linux affected Rust crates plus `journal-log-writer --features serde-api`:
  PASS.
- macOS affected Rust crates plus `journal-log-writer --features serde-api`:
  PASS.
- Windows affected Rust crates plus `journal-log-writer --features serde-api`:
  PASS.
- macOS and Windows Rust writer/read synthetic journal smokes under remote
  `.local/native-smoke/`: PASS.
- Linux stock `journalctl --verify --file` passed for the copied macOS and
  Windows Rust-generated journal files.

Remaining parent blockers:

- SOW-0063 remains open for SOW-0071 runtime-purity separation and native
  FreeBSD runtime execution.
