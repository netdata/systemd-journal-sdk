# SOW-0063 - Cross Platform Portability

## Status

Status: in-progress

Sub-state: native Linux/macOS/Windows validation complete for the reviewed
child portability work; parent remains open until SOW-0071 runtime-purity split
is implemented and reviewed.

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
- SOW-status.md: keep the current/pending SOW index aligned with any lifecycle
  move.

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

## Native Validation Plan

Facts:

- SOW-0067 through SOW-0070 are already in `current/` with reviewed child
  portability work.
- SOW-0071 is open and explicitly keeps runtime-purity separation out of this
  native validation pass.
- The available native validation hosts for this pass are the approved macOS
  and Windows SSH targets from the assigned prompt.

Scope:

- Validate the already-merged portability work on Linux, macOS, and Windows for
  Rust, Go, Python, and Node.js.
- Fix native macOS/Windows portability bugs found by those validations only.
- Do not implement SOW-0071, do not split core SDK runtime-purity layers, and
  do not refactor architecture unless native validation proves a direct
  portability bug.

Remote clone setup:

- Create a bundle from this local branch under `.local/`.
- Copy the bundle to each approved remote host under `/tmp`.
- Recreate the remote validation clone under the approved repository directory
  on each host from the bundle.
- Check out `codex/sow-0063-native-validation`.
- Put language caches under each remote clone's `.local/` directory.

Linux validation:

- Record sanitized OS/runtime versions.
- Run `git diff --check` and `.agents/sow/audit.sh`.
- Run Rust affected SDK tests for reader/writer/facade/journalctl crates.
- Run `go test ./...` from `go/`.
- Run `python3 python/test_all.py` with repository-local Python cache/deps.
- Run `npm test` from `node/` with npm cache under `.local/`.
- Run interoperability smoke after any runtime-behavior fix.

macOS validation:

- Record sanitized OS/runtime versions.
- Run Rust affected tests or record exact build blocker.
- Run Go tests.
- Run Python tests/imports.
- Run Node tests.
- Generate at least one synthetic journal per writer that can execute.
- Copy generated synthetic journals back to Linux under `.local/` and verify
  applicable files with stock `journalctl --verify --file`.

Windows validation:

- Record sanitized OS/runtime versions.
- Use Bash where practical and add the user Rust toolchain directory through
  `$HOME/.cargo/bin` without recording a personal home path.
- Run Rust affected tests or record exact build blocker.
- Run Go tests.
- Run Python tests/imports.
- Run Node tests.
- Generate at least one synthetic journal per writer that can execute.
- Copy generated synthetic journals back to Linux under `.local/` and verify
  applicable files with stock `journalctl --verify --file`.

Evidence handling:

- Use synthetic journal data only.
- Do not read live host journals, `/var/log/journal`, `/run/log/journal`, or
  live `journalctl` without `--file` or a repository-local `--directory`.
- Record only sanitized versions, commands, pass/fail results, generated
  synthetic artifact paths under `.local/`, and exact code blockers.

## Delegation Plan

Implementer:

- Current routing is local implementation. Do not run external implementer
  agents unless the user explicitly changes the routing decision.

Reviewers:

- Use all five read-only reviewers after implementation and local/remote
  validation complete: `llm-netdata-cloud/kimi-k2.6`,
  `llm-netdata-cloud/qwen3.6-plus`, `llm-netdata-cloud/glm-5.1`,
  `llm-netdata-cloud/minimax-m2.7-coder`, and
  `llm-netdata-cloud/mimo-v2.5-pro`.

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
- Moved this SOW to `current/` for native macOS/Windows validation of the
  reviewed child work.
- Recreated approved remote validation clones from a local branch bundle under
  each remote `$HOME/src/systemd-journal-sdk/`, with language caches under each
  clone's `.local/`.
- Recorded sanitized native runtimes:
  - Linux: `Linux 7.0.9-1-MANJARO x86_64`, Rust/Cargo `1.91.1`, Go
    `go1.26.3-X:nodwarf5 linux/amd64`, Python `3.14.5`, Node.js `v26.2.0`,
    npm `11.14.1`, systemd `260 (260.1-2-manjaro)`.
  - macOS: Darwin `25.5.0 arm64`, product `26.5`, Rust/Cargo `1.95.0`
    Homebrew, Go `go1.26.3 darwin/arm64`, Homebrew Python `3.14.5`,
    Node.js `v26.0.0`, npm `11.12.1`.
  - Windows: MSYS `MSYS_NT-10.0-26200 3.6.7 x86_64`, Rust/Cargo `1.94.0`,
    Go `go1.26.0 windows/amd64`, Python `3.12.13`, default Node.js `v22.14.0`,
    default npm `10.9.2`; Node.js package validation used repo-local official
    Node.js `v26.2.0` because the package now requires Node.js `>=22.15.0`.
- Found and repaired native portability debt:
  - Node.js package metadata previously allowed Node.js `>=22.0.0`, but
    `node:zlib` zstd exports are absent in Node.js `v22.14.0`; raised package
    and docs requirement to `>=22.15.0`.
  - Node.js dependency install on Windows attempted native `node-gyp` hooks for
    `node-liblzma`; added `node/.npmrc` with `ignore-scripts=true` because the
    runtime imports only `node-liblzma/wasm/*`.
  - Node.js tests assumed POSIX path separators and stock `journalctl`
    availability; switched path checks to `basename()` and gated stock
    `journalctl` assertions when the tool is absent.
  - Node.js writer directory fsync used POSIX directory descriptors; Windows now
    skips parent-directory fsync while still fsyncing journal files.
  - Python tests and adapter assumed stock `journalctl`, POSIX mode bits, and
    stdlib zstd availability; added target-aware skips only for unavailable
    stock tooling and zstd fixtures.
  - Rust non-Linux Unix stale-lock liveness accepted invalid or wrapped PIDs;
    invalid/non-positive `pid_t` values now count as stale.
  - Rust `journal-registry` path parsing rejected native Windows absolute
    paths; `File::from_path()` and `File::from_raw_path()` now accept native
    absolute paths.
  - Rust `journal-common` realtime-clock test assumed two real `now()` calls
    differ by exactly one microsecond; the test now uses deterministic observed
    timestamps.
  - Rust `journal-log-writer` tests loaded host machine/boot IDs; tests now use
    deterministic IDs so Windows runtime tests do not depend on host identity
    services.
  - Rust `serde-api` depended on a Git package with long upstream paths that
    failed Windows Cargo checkout under repo-local caches; replaced it with a
    small in-crate JSON flattener and coverage.
- Did not implement SOW-0071. Runtime-purity and optional platform-service
  separation remains tracked there.

## Validation

Acceptance criteria evidence:

- Native Linux/macOS/Windows evidence is complete for the reviewed child
  portability work.
- Files written natively on macOS and Windows by Rust, Go, Python, and Node.js
  were copied back under Linux `.local/native-smoke/` and passed stock
  `journalctl --verify --file`.
- SOW-0063 remains `in-progress` because SOW-0071 is still open and because
  this pass did not add native FreeBSD runtime execution.

Tests or equivalent validation:

- Linux:
  - `git diff --check`: PASS.
  - Rust touched-file `rustfmt --edition 2024 --check`: PASS.
  - `cargo test --manifest-path rust/Cargo.toml -p journal-common -p journal-registry -p journal-core -p journal-log-writer -p journal -p journalctl`: PASS.
  - `cargo test --manifest-path rust/Cargo.toml -p journal-log-writer --features serde-api`: PASS.
  - `go test ./...` from `go/` with repo-local Go caches: PASS.
  - `python/test_all.py` with repo-local Python environment: PASS.
  - `npm test` from `node/` with repo-local npm cache: PASS.
  - `tests/interoperability/run_matrix.py --entries 10 --writers go rust python node --readers stock go rust python node`: 104/104 PASS.
  - `tests/interoperability/run_directory_matrix.py --readers stock go rust python node`: `status: PASS`.
- macOS:
  - Rust affected crates plus `journal-log-writer --features serde-api`: PASS.
  - `go test ./...` from `go/`: PASS.
  - `python/test_all.py`: PASS.
  - `node/test/all.js` through `npm test`: PASS.
  - Rust, Go, Python, and Node.js each wrote and read a synthetic journal under
    remote `.local/native-smoke/`: PASS.
- Windows:
  - Rust affected crates plus `journal-log-writer --features serde-api`: PASS.
  - `go test ./...` from `go/`: PASS.
  - `python/test_all.py`: PASS with zstd-specific fixture tests skipped because
    Windows Python `3.12.13` lacks stdlib `compression.zstd`.
  - Default Node.js `v22.14.0` failed as expected because `node:zlib` does not
    export `zstdDecompressSync`; this confirms the corrected `>=22.15.0`
    engine floor.
  - Repo-local official Node.js `v26.2.0` ran `node/test/all.js`: PASS.
  - Rust, Go, Python, and Node.js each wrote and read a synthetic journal under
    remote `.local/native-smoke/`: PASS.
- Cross-OS artifact validation:
  - Copied 4 macOS-generated and 4 Windows-generated synthetic `.journal` files
    back to Linux `.local/native-smoke/`.
  - `journalctl --verify --file` passed for all 8 copied files.

Real-use evidence:

- Native macOS and Windows smoke tests wrote real journal files with each
  language writer and read them back with the same language reader.
- Linux stock `journalctl --verify --file` accepted all copied macOS/Windows
  writer artifacts.
- Linux stock and repository readers accepted all four language writers in the
  closed-file matrix: 104/104 PASS.

Reviewer findings:

- Five read-only whole-SOW reviewers completed after implementation and
  local/remote validation:
  - `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`; no blocking findings.
  - `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`; one non-blocking note that
    `File::from_str()` remains Unix-string oriented while `from_path()` and
    `from_raw_path()` are now native-path safe.
  - `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE`; no blocking
    findings.
  - `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`; maintenance notes about
    package-level `node/.npmrc` `ignore-scripts=true`, artificial POSIX
    filename parsing inside Rust `from_path()`, extra JSON-flattening parity
    coverage, and a stale Python zstd-skip observation.
  - `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`; no blocking
    findings.
- Dispositions:
  - The original `node/.npmrc` `ignore-scripts=true` disposition was superseded
    by SOW-0072. The final Node.js package no longer depends on the full
    `node-liblzma` package; it vendors only the WASM runtime files and keeps
    `.npmrc` as a local development guard, not as the package guarantee.
  - Rust `File::from_str()` remaining slash-oriented is accepted as a
    pre-existing string-parser limitation. Native runtime paths fixed in this
    pass use `File::from_path()`/`File::from_raw_path()` and passed native
    Windows validation. A future API-hardening SOW can decide whether to extend
    the string parser.
  - Rust `from_path()` constructing a filename-only `/{filename}` for internal
    `Status::parse()`/`Source::parse()` is accepted as low-risk because the
    external native path boundary is handled with `Path` APIs and is covered by
    tests.
  - Extra `flatten_json_map()` parity cases are accepted as non-blocking; the
    replacement is feature-gated, tested for nested objects/arrays/originals,
    and passed Linux/macOS/Windows `serde-api` tests.
  - The Python zstd skip note is stale for the current code:
    `test_conformance_manifest()` uses `zstd_available()` and the adapter uses
    `_HAS_ZSTD`/fixture inspection rather than a Windows-only substring skip.

Same-failure scan:

- Same-class fixes were searched across the changed surfaces:
  - Node.js stock `journalctl` assertions now use shared availability gates.
  - Node.js path separator assumptions now use path APIs.
  - Python conformance zstd skips now apply to every fixture path that requires
    zstd when stdlib zstd is unavailable.
  - Rust tests no longer depend on host machine/boot IDs in
    `journal-log-writer` runtime tests.

Sensitive data gate:

- No sensitive runtime data used.
- Remote validation used only deterministic synthetic journal entries.
- Live host journals, `/var/log/journal`, `/run/log/journal`, and live
  `journalctl` without repository-local `--file`/`--directory` were not read.

Artifact maintenance gate:

- AGENTS.md: no update. The existing SOW and runtime-purity rules already cover
  this validation workflow.
- Runtime project skills: no update. The compatibility skill already requires
  cross-language and stock-tool validation discipline.
- Specs: `.agents/sow/specs/product-scope.md` updated for Node.js `>=22.15.0`
  zstd support, Windows parent-directory fsync behavior, and later SOW-0072
  Node.js XZ package hygiene.
- End-user/operator docs: `node/README.md` updated for Node.js `>=22.15.0`,
  bundled WASM-only XZ runtime behavior, and Windows directory fsync scope.
- End-user/operator skills: no output/reference skill exists for this repo
  surface.
- SOW lifecycle: moved this SOW from `pending/` to `current/` with
  `Status: in-progress`; SOW-0071 remains pending/open.
- SOW-status.md: updated to show SOW-0063 current native validation status.

Specs update:

- Updated `.agents/sow/specs/product-scope.md` for the Node.js zstd engine
  floor and platform-specific directory fsync behavior.

Project skills update:

- No project skill update made. The existing compatibility skill remains
  accurate for this validation pass.

End-user/operator docs update:

- Updated `node/README.md` for the Node.js runtime floor, no-native-build
  install behavior, and Windows directory fsync limitation.

End-user/operator skills update:

- No end-user/operator skill exists or was affected.

Lessons:

- Cross-platform support must be proven with runtime tests. Cross-compilation
  catches only part of the problem.
- Engine floors must be validated against the exact oldest claimed runtime.
  Node.js `v22.14.0` is too old for the current zstd API surface.
- Windows validation needs repo-local runtime/tool caches so tests can run
  without changing the host installation.

Follow-up mapping:

- `SOW-0067-20260530-go-cross-platform-portability.md` tracks Go
  implementation.
- `SOW-0068-20260530-rust-cross-platform-portability.md` tracks Rust
  implementation.
- `SOW-0069-20260530-python-cross-platform-portability.md` tracks Python
  implementation.
- `SOW-0070-20260530-node-cross-platform-portability.md` tracks Node.js
  implementation.
- `SOW-0071-20260530-runtime-purity-and-optional-platform-services.md` remains
  the blocker for closing this parent SOW and for stable API release.
- `SOW-0072-20260530-dependency-and-package-hygiene.md` tracks the dependency
  and package hygiene cleanup raised during orchestrator review of this SOW.

## Outcome

Native Linux/macOS/Windows validation for the reviewed child portability work
is complete. SOW-0063 remains in progress because SOW-0071 is still pending and
because native FreeBSD runtime execution was not part of this pass.

## Lessons Extracted

- Record the minimum supported runtime version as an executable contract, not
  documentation only.
- Keep non-Linux stock-systemd assertions gated; the portability contract is
  file-format compatibility plus Linux stock validation of transferred files.
- Keep generated validation artifacts under `.local/` and sanitize remote paths
  in durable records.

## Followup

- Child implementation SOWs: SOW-0067, SOW-0068, SOW-0069, and SOW-0070.
- Required blocker SOW: SOW-0071.
- Cleanup SOW raised by orchestrator review: SOW-0072.
- Remaining parent-scope validation gap: native FreeBSD runtime execution.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and
later testing or use found broken behavior. Use a dated `## Regression -
YYYY-MM-DD` heading at the end of the file. Never prepend regression content
above the original SOW narrative.
