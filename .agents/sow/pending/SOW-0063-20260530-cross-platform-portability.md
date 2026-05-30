# SOW-0063 - Cross Platform Portability

## Status

Status: open

Sub-state: decomposed into per-language child SOWs for parallel worktree
execution; parent closes after child SOWs and shared validation reconcile.

## Requirements

### Purpose

Make the SDK portable across the operating systems required by downstream
consumers, without weakening the systemd journal file-format compatibility,
one-writer safety contract, reader correctness, or ingestion performance goals.

### User Request

The user stated: this SDK must be portable to Linux, FreeBSD, macOS, and
Windows.

### Assistant Understanding

Facts:

- The journal file format itself is byte-oriented and OS-independent.
- The current repository compatibility target remains systemd/systemd v260.1
  for file-format behavior.
- Linux is the only target in this list with stock systemd `journalctl` and
  libsystemd reader validation available as native compatibility oracles.
- Files generated on FreeBSD, macOS, and Windows still must be transferable to
  Linux and pass stock `journalctl --verify --file`, stock reader checks where
  applicable, and the repository cross-language reader matrix.
- Current Go does not build for Windows because `syscall.Flock`,
  `syscall.LOCK_EX`, `syscall.LOCK_NB`, and `syscall.LOCK_UN` are used in the
  writer path.
- Current Rust `journal-core` does not check cleanly for Windows because
  `journal-common` uses `nix::sys::time` / `nix::time` APIs unavailable for the
  checked Windows target.
- Current Python imports POSIX-only `fcntl` from the writer module, which makes
  normal SDK import fail on Windows.
- Current Node.js writer and lock owner detection use Linux `/proc` paths.

Inferences:

- Portability needs explicit platform abstraction, not scattered conditional
  fixes.
- The reader-only path is likely easier to make portable than the writer path,
  but the user requirement covers the SDK, so both readers and writers are in
  scope.
- Live stock-reader compatibility remains a Linux validation requirement; on
  non-Linux targets, the equivalent requirement is same-SDK one-writer/multiple
  readers plus Linux stock validation of files created on those targets.
- Native systemd tooling is not expected on FreeBSD, macOS, or Windows for this
  project unless a test environment explicitly provides it.

Unknowns:

- Whether all current compression dependencies support FreeBSD, macOS, and
  Windows in the exact versions pinned by the project.
- Whether the existing Python runtime requirement can remain unchanged on all
  target operating systems, especially for zstd support.
- Whether the project will use CI-hosted runners, local VMs, or both for
  Windows, FreeBSD, and macOS execution evidence.

### Acceptance Criteria

- Rust, Go, Node.js, and Python SDK packages build/import on Linux, FreeBSD,
  macOS, and Windows.
- Single-file readers and writers work on all four operating systems for:
  regular/compact format, uncompressed DATA, zstd/xz/lz4 DATA compression where
  dependency support exists, FSS sealed/unsealed files, RAW/JOURNALD/JOURNAL-APP
  field policies, binary fields, repeated fields, and final states
  online/offline/archived.
- Directory readers and writers work on all four operating systems, including
  rotation and retention, while preserving the existing active/archive naming
  contracts.
- One-writer/multiple-reader protection works on every target. The lock
  mechanism may be platform-specific, but the public contract must remain the
  same.
- Files written on each target OS by each language can be read by every SDK
  reader and file-backed journalctl rewrite on every target OS where that
  language runtime is supported.
- Files written on FreeBSD, macOS, and Windows are copied or otherwise made
  available to Linux validation and pass stock `journalctl --verify --file` and
  relevant stock-reader checks.
- Linux live compatibility remains covered by stock `journalctl --file`,
  `journalctl --follow`, and libsystemd reader tests.
- Non-Linux live compatibility is covered by repository readers and writers with
  the same one-writer/multiple-reader semantics and explicit final Linux stock
  validation.
- Platform-specific behavior is documented in SDK docs and product specs.
- CI or reproducible local scripts run an OS matrix for Linux, FreeBSD, macOS,
  and Windows without writing outside this repository except `/tmp`.

## Analysis

Sources checked:

- `go/journal/writer.go`
- `go/journal/lock.go`
- `go/journal/mmap_other.go`
- `rust/src/crates/journal-core/src/file/lock.rs`
- `rust/src/crates/journal-core/src/file/file.rs`
- `rust/src/crates/journal-common/src/time.rs`
- `python/journal/__init__.py`
- `python/journal/writer.py`
- `python/journal/lock.py`
- `node/src/lib/lock.js`
- `node/src/lib/writer.js`
- `go/README.md`
- `rust/README.md`
- `python/README.md`
- `node/README.md`

Current state:

- Go writer calls POSIX `flock` directly during close, archive, lock, and
  unlock paths: `go/journal/writer.go:465`, `go/journal/writer.go:506`,
  `go/journal/writer.go:646`, `go/journal/writer.go:653`.
- Go writer lock stale-owner detection reads Linux `/proc`:
  `go/journal/lock.go:133` and `go/journal/lock.go:141`.
- Go has a non-Unix mmap/read-write fallback in `go/journal/mmap_other.go`,
  but the writer package still fails Windows compilation before that fallback is
  useful.
- A direct Windows cross-check failed with:
  `GOOS=windows GOARCH=amd64 go test ./...` from `go/`, reporting undefined
  `syscall.Flock` and lock constants.
- Rust writer lock stale-owner detection reads Linux `/proc`:
  `rust/src/crates/journal-core/src/file/lock.rs:129` and
  `rust/src/crates/journal-core/src/file/lock.rs:135`.
- A direct Windows cross-check failed with:
  `cargo check --manifest-path rust/Cargo.toml -p journal-core --target x86_64-pc-windows-gnu`,
  reporting unresolved `nix::sys::time::TimeValLike` and `nix::time::ClockId`.
- Python writer imports POSIX-only `fcntl` at module import:
  `python/journal/writer.py:6`.
- Python writer uses POSIX directory-open and advisory lock APIs:
  `python/journal/writer.py:1095` and `python/journal/writer.py:1108`.
- Python lock stale-owner detection reads Linux `/proc`:
  `python/journal/lock.py:110` and `python/journal/lock.py:117`.
- Node.js lock stale-owner detection reads Linux `/proc`:
  `node/src/lib/lock.js:135` and `node/src/lib/lock.js:143`.
- Node.js writer default boot ID loading reads Linux `/proc`:
  `node/src/lib/writer.js:1054`.
- Node.js README already documents that no native mmap dependency is loaded by
  the runtime path, so portable read/write fallback is expected to stay
  Buffer/file-I/O based for Node.js.

Risks:

- Weakening locks for non-Linux targets can violate the one-writer contract and
  cause journal corruption.
- Replacing mmap with read/write fallbacks can reduce performance, especially in
  Go/Rust writer hot paths and readers.
- Directory sync and rename semantics differ by operating system; treating them
  as identical can produce durability claims that are false on some targets.
- Windows path behavior, sharing modes, delete/rename semantics, and file
  locking differ substantially from POSIX.
- FreeBSD/macOS do not provide native systemd stock reader tooling by default,
  so Linux stock validation must consume cross-OS generated artifacts.
- Cross-compilation alone is insufficient; runtime tests are required.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Current SDK code contains Linux/POSIX assumptions in writer locks, file locks,
  process-owner stale-lock detection, directory sync, mmap strategy, boot ID
  loading, and benchmark instrumentation. These assumptions block Windows
  builds today and likely make FreeBSD/macOS support partial or unproven.

Evidence reviewed:

- Go Windows build failure from `GOOS=windows GOARCH=amd64 go test ./...`.
- Rust Windows cross-check failure from
  `cargo check --manifest-path rust/Cargo.toml -p journal-core --target x86_64-pc-windows-gnu`.
- Source evidence listed under Analysis.
- Product scope and project compatibility rules under `.agents/sow/specs/` and
  `.agents/skills/project-journal-compatibility/SKILL.md`.

Affected contracts and surfaces:

- Rust, Go, Node.js, and Python public SDK imports/builds.
- Direct file readers and writers.
- Directory readers and writers.
- File-backed journalctl rewrites.
- Locking and one-writer safety.
- Live read/follow behavior.
- Rotation, retention, archive rename, and directory sync behavior.
- Compression dependency support.
- CI, interoperability matrix, benchmark scripts, and documentation.

Existing patterns to reuse:

- Existing cross-language interoperability runners under `tests/interoperability/`.
- Existing live-concurrency harness under `tests/conformance/live/`.
- Existing writer lockfile format `systemd-journal-sdk-lock-v1`.
- Existing Go `mmap_other.go` fallback pattern for non-Unix targets.
- Existing Node.js Buffer/file-I/O runtime path.
- Existing SOW validation gates for compact, compression, mixed directory,
  live, verify, and byte identity.

Risk and blast radius:

- High. This touches all languages, platform-specific filesystem behavior,
  concurrency protection, and validation infrastructure.
- Linux Netdata performance must not regress while adding portability.
- Non-Linux portability fixes must not silently reduce compatibility guarantees
  for Linux stock readers.
- Public docs must not overclaim stock systemd validation on targets where
  stock systemd tooling is absent.

Sensitive data handling plan:

- Use deterministic synthetic journal fixtures only.
- Do not read host live journals.
- Do not record real logs, credentials, bearer tokens, SNMP communities,
  customer identifiers, personal data, private endpoints, or proprietary
  incident details.
- CI artifacts must contain only synthetic journals generated by repository
  tests.

Implementation plan:

1. Portability audit and platform contract.
   - Define per-platform expectations for locking, stale-lock detection,
     boot/machine IDs, directory sync, mmap/read fallback, file sharing, rename,
     permissions, and live-reader semantics.
   - Update specs before implementation.
2. Go portability.
   - Split POSIX `flock` and `/proc` code behind build-tagged platform files.
   - Add Windows and BSD/macOS lock/process owner implementations or safe
     fallbacks with equivalent one-writer protection.
   - Keep Linux hot-path performance unchanged.
3. Rust portability.
   - Replace or gate Linux-only `nix` time and `/proc` assumptions.
   - Add platform modules for lock owner identity, boot ID, file lock, directory
     sync, and mmap strategy.
   - Keep Linux journal-core performance unchanged.
4. Python portability.
   - Stop importing POSIX-only `fcntl` on Windows.
   - Add platform-specific lock and directory sync implementations.
   - Validate Python mmap behavior on Windows and provide read/write fallback
     where needed.
5. Node.js portability.
   - Replace Linux `/proc` stale-owner and boot-ID assumptions with portable
     platform modules.
   - Preserve Buffer/file-I/O runtime path without native mmap dependencies.
6. Cross-platform validation infrastructure.
   - Add OS matrix scripts and/or CI jobs for Linux, FreeBSD, macOS, and
     Windows.
   - Generate cross-OS writer artifacts and validate them on Linux with stock
     systemd tools.
7. Documentation and release readiness.
   - Update SDK docs, product specs, and any project skills affected by the new
     portability workflow.

Validation plan:

- Per-language build/import tests on Linux, FreeBSD, macOS, and Windows.
- Per-language unit tests on every target where the language runtime is
  supported.
- Cross-language single-file interoperability matrix on every target.
- Directory reader/writer matrix on every target.
- Live one-writer/multiple-reader matrix on every target using repository
  readers; Linux additionally uses stock `journalctl` and stock libsystemd.
- Cross-OS artifact validation: journals written on FreeBSD, macOS, and Windows
  must pass Linux stock `journalctl --verify --file` and relevant stock-reader
  checks.
- Byte-identity tests for deterministic uncompressed files where the platform
  can produce deterministic metadata inputs.
- Compression/FSS/compact/mixed-directory verification on all targets or
  explicit, documented dependency exclusions with user approval.
- Performance regression checks on Linux for Rust and Go writer hot paths after
  portability abstractions land.
- `.agents/sow/audit.sh` and `git diff --check` before close.

Artifact impact plan:

- AGENTS.md: likely no update unless repository-wide portability workflow rules
  change.
- Runtime project skills: likely update `project-journal-compatibility` if
  cross-platform validation becomes a mandatory compatibility gate.
- Specs: update `.agents/sow/specs/product-scope.md` with supported operating
  systems and validation semantics.
- End-user/operator docs: update Rust, Go, Node.js, and Python README/API docs
  with supported platforms and any platform-specific limitations.
- End-user/operator skills: no output/reference skill expected unless docs/spec
  changes create a reusable operator workflow.
- SOW lifecycle: this SOW may need child SOWs by language or platform after the
  audit phase, but no implementation should be marked complete until the full
  portability matrix is proven.
- SOW-status.md: add this SOW to Pending.

Open-source reference evidence:

- No external open-source repositories were checked while creating this SOW.
  Implementation should consult official platform documentation and current
  language runtime documentation before choosing platform-specific locking,
  mmap, directory-sync, and rename strategies.

Open decisions:

- Resolved by user: the SDK must be portable to Linux, FreeBSD, macOS, and
  Windows.
- Future implementation may return with evidence if a dependency prevents a
  feature slice on a target, but the default requirement is full support.

## Implications And Decisions

1. 2026-05-30 platform support requirement
   - Decision: Linux, FreeBSD, macOS, and Windows are required SDK targets.
   - Implication: portability is not best-effort and cannot be documented as a
     limitation.
   - Risk: this requires runtime validation on non-Linux targets, not just
     cross-compilation.

2. 2026-05-30 stock systemd validation interpretation
   - Decision: stock systemd validation remains mandatory where stock systemd
     tooling is available, primarily Linux. Files generated on non-Linux
     targets must still be validated by stock systemd tooling after transfer to
     Linux.
   - Implication: non-Linux support is file-format portability, not native
     systemd daemon/tooling availability.
   - Risk: docs must avoid implying native `journalctl`/libsystemd availability
     on operating systems where this project does not provide it.

## Plan

1. Audit and spec the platform contract.
   - Scope: every POSIX/Linux-specific path and every required OS behavior.
   - Risk: missing a hidden dependency such as benchmark `/proc` reads or test
     assumptions.

2. Build/import gates.
   - Scope: make every language build/import on all four targets.
   - Risk: compiling does not prove runtime file semantics.

3. Reader portability.
   - Scope: closed-file and directory reads, compression, FSS verification,
     file-backed journalctl rewrites.
   - Risk: mmap and path semantics differ by platform.

4. Writer portability.
   - Scope: direct writer, directory writer, locks, live publication, rotation,
     retention, compression, FSS, compact format.
   - Risk: one-writer safety and durability semantics are platform-specific.

5. Cross-OS interoperability and Linux stock validation.
   - Scope: each OS writes artifacts; Linux stock tools validate them; every SDK
     reader reads them.
   - Risk: artifact movement must preserve bytes and metadata assumptions must
     be deterministic.

6. Documentation, review, and close.
   - Scope: docs/specs/skills updated, reviewers run whole-SOW, audit clean,
     committed rollback point.

## Delegation Plan

Implementer:

- Current routing is local implementation. Do not run external implementer
  agents unless the user explicitly changes the routing decision.

Reviewers:

- Use read-only reviewers from the approved pool after implementation and local
  validation complete: `llm-netdata-cloud/minimax-m2.7-coder`,
  `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/qwen3.6-plus`, and
  `llm-netdata-cloud/glm-5.1`. Skip `llm-netdata-cloud/mimo-v2.5-pro` while it
  remains out of quota.

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

- If a target OS is unavailable locally, record the gap and use CI or an
  explicitly approved VM path.
- If a dependency does not support a target, stop with evidence and options
  before weakening the portability requirement.
- If a platform lock/durability implementation cannot match the current public
  contract, stop with evidence and user decision options.
- If portability abstractions regress Linux Rust/Go writer performance, either
  fix the regression or return with benchmark evidence before closing.

## Execution Log

### 2026-05-30

- Created this pending SOW after the user required SDK portability to Linux,
  FreeBSD, macOS, and Windows.
- Recorded current blockers from local source inspection and direct Go/Rust
  Windows build checks.
- Split implementation into per-language child SOWs so the user can spawn
  independent implementation agents in isolated git worktrees:
  `SOW-0067` for Go, `SOW-0068` for Rust, `SOW-0069` for Python, and
  `SOW-0070` for Node.js.

## Validation

Acceptance criteria evidence:

- Pending implementation.

Tests or equivalent validation:

- SOW creation evidence only:
  - `GOOS=windows GOARCH=amd64 go test ./...` currently fails due to
    `syscall.Flock` and POSIX lock constants.
  - `cargo check --manifest-path rust/Cargo.toml -p journal-core --target x86_64-pc-windows-gnu`
    currently fails due to Windows target issues in `journal-common` time code.

Real-use evidence:

- Pending implementation.

Reviewer findings:

- Pending implementation and whole-SOW review.

Same-failure scan:

- Initial scan found platform-specific assumptions in Go, Rust, Python, and
  Node.js paths listed under Analysis.

Sensitive data gate:

- No sensitive runtime data used. Only synthetic code/build evidence recorded.

Artifact maintenance gate:

- AGENTS.md: no update during SOW creation.
- Runtime project skills: no update during SOW creation.
- Specs: pending implementation; product scope must be updated during this SOW.
- End-user/operator docs: pending implementation; language README/API docs must
  be updated during this SOW.
- End-user/operator skills: no output/reference skill affected during SOW
  creation.
- SOW lifecycle: created as `Status: open` under `.agents/sow/pending/`.
- SOW-status.md: updated to list this SOW as pending.

Specs update:

- Pending implementation.

Project skills update:

- Pending implementation decision. `project-journal-compatibility` likely needs
  a cross-platform validation gate after this SOW lands.

End-user/operator docs update:

- Pending implementation.

End-user/operator skills update:

- Pending implementation.

Lessons:

- Cross-platform support must be proven with runtime tests. Cross-compilation
  catches only part of the problem.

Follow-up mapping:

- `SOW-0067-20260530-go-cross-platform-portability.md` tracks Go
  implementation.
- `SOW-0068-20260530-rust-cross-platform-portability.md` tracks Rust
  implementation.
- `SOW-0069-20260530-python-cross-platform-portability.md` tracks Python
  implementation.
- `SOW-0070-20260530-node-cross-platform-portability.md` tracks Node.js
  implementation.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

- Child implementation SOWs: SOW-0067, SOW-0068, SOW-0069, and SOW-0070.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and
later testing or use found broken behavior. Use a dated `## Regression -
YYYY-MM-DD` heading at the end of the file. Never prepend regression content
above the original SOW narrative.
