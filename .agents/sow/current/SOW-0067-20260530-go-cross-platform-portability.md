# SOW-0067 - Go Cross Platform Portability

## Status

Status: in-progress

Sub-state: implemented; ready for orchestrator review; child of SOW-0063.

## Requirements

### Purpose

Make the Go SDK reader, writer, directory writer, and file-backed journalctl
paths portable to Linux, FreeBSD, macOS, and Windows without weakening Linux
performance or journal compatibility.

### User Request

The user requested SDK portability to Linux, FreeBSD, macOS, and Windows, and
approved parallel worktrees for independent SOWs.

### Assistant Understanding

Facts:

- This SOW covers Go only.
- SOW-0063 recorded that Go currently fails Windows compilation because
  `syscall.Flock`, `syscall.LOCK_EX`, `syscall.LOCK_NB`, and
  `syscall.LOCK_UN` are used in writer paths.
- SOW-0063 recorded Linux `/proc` assumptions in Go stale-lock owner detection.
- Go already has a non-Unix mmap/read-write fallback, but common writer code
  fails before that fallback is useful on Windows.

Inferences:

- The correct implementation shape is build-tagged platform helpers for locks,
  process identity, directory sync, and any target-specific file behavior.
- Linux hot paths must stay as close to current behavior as possible because
  Go writer/reader performance is a Netdata gate.

Unknowns:

- Which non-Linux runtime environments are available locally for execution.

### Acceptance Criteria

- `go test ./...` passes on Linux from `go/`.
- `GOOS=windows GOARCH=amd64 go test ./...` compiles Go packages from `go/`.
- FreeBSD and macOS checks are added or exact local blockers are recorded.
- Go writer locking preserves one-writer behavior on supported targets.
- Non-Linux generated files have a reproducible path for Linux stock
  `journalctl --verify --file` validation.
- Linux Go performance does not regress unless measured and explicitly accepted.
- Specs/docs describe Go platform behavior.

## Analysis

Sources checked:

- `go/journal/writer.go`
- `go/journal/lock.go`
- `go/journal/mmap_other.go`
- `.agents/sow/pending/SOW-0063-20260530-cross-platform-portability.md`
- `.agents/skills/project-journal-compatibility/SKILL.md`

Current state:

- Go uses POSIX file locking from shared writer code.
- Go stale-lock owner detection reads Linux `/proc`.
- Non-Unix mmap fallback exists but is blocked by compile failures elsewhere.

Risks:

- Weak locking can corrupt journal files under accidental multiple writers.
- Abstractions in hot paths can reduce ingestion performance.
- Windows sharing, delete, and rename semantics differ from POSIX.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Go portability is blocked by POSIX and Linux assumptions in file locking and
  stale-owner detection, not by the journal format model itself.

Evidence reviewed:

- SOW-0063 build-failure and source evidence.
- Project compatibility skill one-writer/multiple-reader and stock-validation
  requirements.

Affected contracts and surfaces:

- Go SDK imports/builds.
- Go writer, directory writer, reader, and journalctl rewrite.
- Lockfile behavior and retention/rotation paths.
- Linux performance benchmark expectations.

Existing patterns to reuse:

- Go build tags and existing `mmap_other.go`.
- Existing lockfile format `systemd-journal-sdk-lock-v1`.
- Existing interoperability and lock matrix runners.

Risk and blast radius:

- Medium-high for Go users; high if Linux performance regresses.

Sensitive data handling plan:

- Use synthetic fixtures only; do not read host live journals or record raw log
  payloads.

Implementation plan:

1. Split Go platform assumptions into build-tagged helpers.
2. Implement platform locks and owner/boot/directory helpers.
3. Run Linux tests and cross-target checks.
4. Update docs/specs and SOW validation.

Validation plan:

- Linux Go tests.
- Windows cross-compilation with caches under `.local/`.
- FreeBSD/macOS checks or blocker evidence.
- Relevant lock/interoperability tests.
- Benchmark smoke check if hot paths change.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: no update expected.
- Specs: update cross-platform behavior.
- End-user/operator docs: update Go docs.
- End-user/operator skills: no update expected.
- SOW lifecycle: child of SOW-0063.
- SOW-status.md: orchestrator reconciliation required; this worktree prompt
  says not to edit it.

Open-source reference evidence:

- Official Go build-constraint documentation checked for `//go:build`,
  GOOS/GOARCH file selection, and the `unix` build tag.
- Microsoft `LockFileEx` documentation checked for non-blocking exclusive
  byte-range locking and explicit unlock behavior.
- Microsoft `GetProcessTimes` documentation checked for Windows process
  creation-time stale-owner tokens.
- Local mirrored reference implementations checked for Windows file-locking
  patterns; no code copied:
  - `grafana/loki @ 1863c893a303`
    `vendor/github.com/gofrs/flock/flock_windows.go:68`
  - `grafana/cortex-tools @ 960678bd3e1d`
    `vendor/go.etcd.io/etcd/client/pkg/v3/fileutil/lock_windows.go:29`

Open decisions:

- None. User approved parallel worktree execution.

## Implications And Decisions

1. 2026-05-30: This SOW is assigned to an isolated worktree. It should not edit
   other language implementations except shared specs/docs/tests required by the
   Go portability contract.

## Plan

1. Isolate Go platform code.
2. Implement portable lock and identity helpers.
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

- Created as Go-only child SOW under SOW-0063 for parallel worktree execution.
- Confirmed user-authorized parallel implementation routing; AGENTS.md
  external-implementer exception applies for this worktree.
- Split Go writer file locks, open/share behavior, stale-lock owner identity,
  host boot ID loading, and directory sync into build-tagged platform helpers.
- Preserved Linux behavior with `/proc` boot/process-start owner checks,
  non-blocking POSIX `flock`, Unix directory fsync, and mmap-backed reader and
  writer paths.
- Added Windows writer/read open helpers with delete-sharing, Windows
  `LockFileEx` non-blocking byte-range locks outside journal data, and process
  creation-time stale-owner checks.
- Added non-Linux stock-tool test gates so pure SDK tests can run on Windows
  without claiming stock systemd tooling exists there.
- Updated Go docs/API and product scope with platform behavior. `SOW-status.md`
  intentionally not edited per the worktree prompt; orchestrator will
  reconcile status.

## Validation

Acceptance criteria evidence:

- Linux `go test ./...` from `go/` with `.local` Go caches: PASS.
- Windows exact command `GOOS=windows GOARCH=amd64 go test ./...` from `go/`
  with `.local` Go caches: PASS in the local Windows runner after non-Linux
  stock-tool tests were gated.
- FreeBSD compile check
  `GOOS=freebsd GOARCH=amd64 go test -exec=true ./...`: PASS.
- macOS compile check `GOOS=darwin GOARCH=amd64 go test -exec=true ./...`:
  PASS.
- Direct FreeBSD execution on this Linux host remains unavailable:
  `GOOS=freebsd GOARCH=amd64 go test ./...` fails when the host attempts to
  execute target test binaries with `signal: segmentation fault`.
- Direct macOS execution on this Linux host remains unavailable:
  `GOOS=darwin GOARCH=amd64 go test ./...` fails with `exec format error`.
- Go writer lock evidence: `TestWriterLockRejectsSecondWriter` passed as part
  of Linux and Windows `go test ./...`.
- Non-Linux generated-file validation path: writers produce normal `.journal`
  files on every target; Linux stock verification remains the documented
  transfer/validation oracle in `go/README.md` and
  `.agents/sow/specs/product-scope.md`.

Tests or equivalent validation:

- `git diff --check`: PASS.
- `.agents/sow/audit.sh`: PASS.
- Windows compile/runtime command: PASS as above.
- FreeBSD/macOS compile commands with `-exec=true`: PASS as above.
- Writer smoke benchmark driver:
  `go run ./internal/testcmd/writer_core_bench -output ../.local/sow-0067-writer-smoke.journal -rows 1000 -format regular -surface direct -api-mode raw-payload -final-state offline`
  completed with `errors: []`, 1,000 records, 32 fields per row.
- Reader smoke benchmark driver:
  `go run ./internal/testcmd/reader_core_bench -input ../.local/sow-0067-writer-smoke.journal -surface file -mode sdk-payloads -direction forward -bounds snapshot -mmap-strategy mmap -loops 1`
  completed with `errors: []`, 1,000 records, 32,000 fields.
- Cross-SDK lock matrix attempted with
  `python3 tests/interoperability/run_lock_matrix.py --entries 20 --delay-ms 1`;
  blocked before lock assertions because the Node writer failed to start with
  `MODULE_NOT_FOUND` from `node/src/lib/lz4-block.js`. This is outside this
  Go-only SOW and is recorded for orchestrator follow-up.

Real-use evidence:

- Linux Go tests exercised direct writer create/open/archive/close paths,
  directory writer rotation/retention paths, reader paths, file-backed
  journalctl rewrite tests, stock `journalctl` checks, and sealed writer checks.
- Windows `go test ./...` exercised pure Go SDK reader/writer tests under the
  local Windows runner while stock systemd checks were skipped as Linux-only.
- Generated smoke file:
  `.local/sow-0067-writer-smoke.journal`, 1,000 synthetic entries, read back by
  the Go reader benchmark driver. Scratch artifact is intentionally under
  `.local/` and not staged.

Reviewer findings:

- Not run by this implementation agent per prompt. Whole-SOW read-only
  reviewers are orchestrator-owned after this worktree is merged/reviewed.

Same-failure scan:

- `rg` confirmed shared Go code no longer imports `syscall.Flock` or references
  `LOCK_EX`, `LOCK_NB`, or `LOCK_UN`; those are isolated in
  `go/journal/file_lock_unix.go`.
- `rg` confirmed Linux `/proc/sys/kernel/random/boot_id` reads are isolated in
  `go/journal/boot_id_linux.go` and `go/journal/lock_owner_linux.go`.
- Writer open paths now call `openWriterFile` at `go/journal/writer.go:141`
  and `go/journal/writer.go:188`.
- Reader/directory helper paths now call `openReaderFile` at
  `go/journal/reader.go:402`, `go/journal/log.go:1001`, and
  `go/journal/log.go:1014`.

Sensitive data gate:

- PASS. Only synthetic journal payloads and durable sanitized evidence were
  used. No host live journals were read or probed.

Artifact maintenance gate:

- `AGENTS.md`: not changed; routing exception already recorded in this SOW and
  no project-wide workflow rule changed.
- Runtime project skills: not changed; no reusable workflow change beyond this
  SOW.
- Specs: updated `.agents/sow/specs/product-scope.md` with Go platform lock and
  identity behavior.
- End-user/operator docs: updated `go/README.md` and `go/API.md`.
- End-user/operator skills: none affected.
- SOW lifecycle: moved from `pending/` to `current/`, status set to
  `in-progress`, sub-state set to implemented/ready for orchestrator review.
- `SOW-status.md`: intentionally not edited per assigned prompt to reduce merge
  conflicts; orchestrator reconciliation required.

Lessons extracted:

- Cross-target `go test` on this workstation may execute Windows binaries via a
  local Windows runner, but FreeBSD/macOS target binaries cannot execute on the
  Linux host. Use `go test -exec=true` as a compile check for those targets
  unless native runners are available.
- Stock `journalctl` tests must be gated to Linux. Non-Linux test success must
  not imply stock systemd tooling exists on that target.
- Windows byte-range file locks must avoid journal byte ranges because
  `LockFileEx` can deny reads of the locked region; this implementation locks a
  high offset outside valid journal data.

Follow-up mapping:

- Parent umbrella: `SOW-0063-20260530-cross-platform-portability.md`.
- Native FreeBSD/macOS runtime execution remains parent/orchestrator scope.
- Cross-SDK lock matrix failure is blocked by the Node writer dependency
  startup error and should be reconciled by the orchestrator or the Node
  portability SOW, not this Go-only SOW.
