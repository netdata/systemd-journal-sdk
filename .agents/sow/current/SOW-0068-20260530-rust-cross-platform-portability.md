# SOW-0068 - Rust Cross Platform Portability

## Status

Status: in-progress

Sub-state: implemented; ready for orchestrator review.

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
  non-Unix uses a process-local `Instant` baseline.
- Replaced Rust writer lock Linux `/proc` assumptions with platform helpers:
  Linux keeps `/proc/<pid>/stat` start-time verification, FreeBSD/macOS use
  same-boot PID liveness, and Windows uses `OpenProcess` plus
  `WaitForSingleObject`.
- Added FreeBSD identity support for common `machine-id` paths and FreeBSD
  boot ID derivation from `sysctl kern.boottime`.
- Kept Linux mmap hot paths intact and added a non-Unix fallback for header
  rewrite during sync.
- Added non-Unix no-op SIGBUS and directory-sync hooks where Unix-only
  mechanisms do not exist.
- Updated `rust/README.md` and `.agents/sow/specs/product-scope.md` with Rust
  platform behavior and non-Linux validation limits.
- Did not edit `SOW-status.md`; status reconciliation is left to the
  orchestrator per the worktree prompt.

## Validation

Acceptance criteria evidence:

- Linux Rust tests passed for affected crates:
  `journal-common`, `journal-core`, `journal-log-writer`, `journal`, and
  `journalctl`.
- Windows Rust target checks passed for SDK crates in scope:
  `journal-core`, `journal-log-writer`, `journal`, and `journalctl`,
  including `--tests`.
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
  `CARGO_HOME="$PWD/.local/cargo-home" CARGO_TARGET_DIR="$PWD/.local/cargo-target" cargo check --manifest-path rust/Cargo.toml -p journal-core --target x86_64-pc-windows-gnu`
- Succeeded:
  `CARGO_HOME="$PWD/.local/cargo-home" CARGO_TARGET_DIR="$PWD/.local/cargo-target" cargo check --manifest-path rust/Cargo.toml -p journal-log-writer --target x86_64-pc-windows-gnu`
- Succeeded:
  `CARGO_HOME="$PWD/.local/cargo-home" CARGO_TARGET_DIR="$PWD/.local/cargo-target" cargo check --manifest-path rust/Cargo.toml -p journal --target x86_64-pc-windows-gnu`
- Succeeded:
  `CARGO_HOME="$PWD/.local/cargo-home" CARGO_TARGET_DIR="$PWD/.local/cargo-target" cargo check --manifest-path rust/Cargo.toml -p journalctl --target x86_64-pc-windows-gnu`
- Succeeded:
  `CARGO_HOME="$PWD/.local/cargo-home" CARGO_TARGET_DIR="$PWD/.local/cargo-target" cargo check --manifest-path rust/Cargo.toml -p journal-core -p journal-log-writer -p journal -p journalctl --tests --target x86_64-pc-windows-gnu`
- Succeeded:
  `CARGO_HOME="$PWD/.local/cargo-home" CARGO_TARGET_DIR="$PWD/.local/cargo-target" cargo test --manifest-path rust/Cargo.toml -p journal-common -p journal-core -p journal-log-writer -p journal -p journalctl`
  Result summary: 22 `journal` tests, 31 `journal-common` tests, 63
  `journal-core` tests, 2 `journal-log-writer` unit tests, 48
  `journal-log-writer` integration tests, 9 `journalctl` tests, and applicable
  doctests passed.
- Succeeded:
  Rust-only livewriter lock contention and stale-lock recovery probe using
  `.local/interoperability/bin/rust-livewriter`; both generated journals passed
  `journalctl --verify --file`.
- Succeeded:
  writer smoke:
  `cargo run --manifest-path rust/Cargo.toml -p writer_core_bench -- --output "$PWD/.local/fedcba9876543210fedcba9876543210/system.journal" --rows 1000 --format regular --surface direct --api-mode raw-payload --final-state offline`
  Result: 1000 rows, 32 fields per row, regular direct writer, offline close,
  `append_rows_per_second` about `41328`, `errors: []`.
- Succeeded:
  `journalctl --verify --file "$PWD/.local/fedcba9876543210fedcba9876543210/system.journal"`
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

- External reviewers were not run in this implementation worktree, per the
  prompt. Whole-SOW reviewer pass is left to the orchestrator.

Same-failure scan:

- `rg -n "\bnix::|nix =" rust/src/crates/journal-common rust/src/crates/journal-core rust/src/crates/journal-log-writer rust/src/journal rust/src/cmd/journalctl rust/Cargo.toml`
  returned no matches after removing `nix` from the affected crates.
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
- SOW lifecycle: moved this SOW from `pending/open` to
  `current/in-progress` and left it ready for orchestrator review, not
  completed.
- `SOW-status.md`: intentionally not updated in this worktree because the
  prompt required leaving status reconciliation to the orchestrator.

Lessons extracted:

- The Rust SDK crates can be made Windows-target clean without weakening Linux
  `/proc` start-time lock validation by isolating liveness checks per target.
- Full-workspace Windows target checking is currently limited by
  `journal-engine`'s native `lz4-sys` build chain, which is separate from the
  public Rust SDK crates validated here.

Follow-up mapping:

- Parent umbrella: `SOW-0063-20260530-cross-platform-portability.md`.
- Orchestrator follow-up: run whole-SOW read-only reviewers and reconcile this
  SOW into `SOW-status.md`.
- Parent/CI follow-up: collect runtime evidence on FreeBSD, macOS, and Windows
  machines or runners, then validate generated files on Linux with stock
  systemd tooling.
- Separate portability follow-up if full Rust workspace portability is desired:
  decide whether `journal-engine` keeps `foyer`/native `lz4-sys` for
  non-public indexing/cache paths or receives a target-specific dependency
  configuration.
