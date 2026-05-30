# SOW-0070 - Node Cross Platform Portability

## Status

Status: in-progress

Sub-state: implemented; ready for orchestrator review.

## Requirements

### Purpose

Make the Node.js SDK reader, writer, directory writer, and file-backed
journalctl paths portable to Linux, FreeBSD, macOS, and Windows without native
mmap or systemd/libjournal dependencies.

### User Request

The user requested SDK portability to Linux, FreeBSD, macOS, and Windows, and
approved parallel worktrees for independent SOWs.

### Assistant Understanding

Facts:

- This SOW covers Node.js only.
- SOW-0063 recorded that Node.js stale-lock owner detection reads Linux
  `/proc`.
- SOW-0063 recorded that Node.js default boot ID loading reads Linux `/proc`.
- Node.js intentionally avoids native mmap in the current runtime path.

Inferences:

- Node.js portability mainly needs portable lock owner and boot/process helpers.
- No native addon should be introduced for mmap or systemd access.

Unknowns:

- Whether all Node.js compression dependencies support every target in the
  accepted runtime policy.
- Which non-Linux runtime environments are available locally for execution.

### Acceptance Criteria

- Node.js tests pass on Linux for affected reader/writer/facade paths.
- Node.js import and core read/write paths are portable by construction to
  Windows, FreeBSD, and macOS, with runtime checks where available.
- Node.js writer locking preserves one-writer behavior on supported targets.
- Boot ID and process-owner behavior no longer assumes Linux `/proc`.
- No native mmap or systemd/libjournal dependency is introduced.
- Specs/docs describe Node.js platform behavior.

## Analysis

Sources checked:

- `node/src/lib/lock.js`
- `node/src/lib/writer.js`
- `node/README.md`
- `.agents/sow/pending/SOW-0063-20260530-cross-platform-portability.md`
- `.agents/skills/project-journal-compatibility/SKILL.md`

Current state:

- Node.js lock stale-owner detection uses Linux `/proc`.
- Node.js default boot ID loading uses Linux `/proc`.
- Node.js already uses Buffer/file-I/O rather than native mmap.

Risks:

- Weak lock fallbacks can allow concurrent writers.
- Platform fallbacks can alter generated metadata and break parity.
- Compression dependencies must not load native code in forbidden runtime paths.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Node.js portability is blocked by Linux `/proc` assumptions in lock owner
  detection and boot ID loading. The main I/O model is already comparatively
  portable.

Evidence reviewed:

- SOW-0063 Node.js `/proc` source evidence.
- Project compatibility skill no-native-runtime and one-writer requirements.

Affected contracts and surfaces:

- Node.js package import/build.
- Node.js writer, directory writer, reader, journalctl rewrite, and locking.
- Compression dependency runtime policy.
- Platform docs/specs.

Existing patterns to reuse:

- Existing Node.js Buffer/file-I/O runtime path.
- Existing lockfile format `systemd-journal-sdk-lock-v1`.
- Existing Node.js tests and shared interoperability runners.

Risk and blast radius:

- Medium. Node.js is slower than Rust/Go today but must remain correct and
  portable.

Sensitive data handling plan:

- Use synthetic fixtures only; do not read host live journals or record raw log
  payloads.

Implementation plan:

1. Replace `/proc` boot/process assumptions with platform helpers.
2. Add portable lock behavior.
3. Validate import/read/write behavior and dependency runtime policy.
4. Update docs/specs and SOW validation.

Validation plan:

- Linux Node.js tests for affected paths.
- Platform checks where target runtimes are available.
- Static/source checks proving no native mmap or systemd/libjournal runtime
  dependency was introduced.
- Relevant shared conformance/interoperability tests on Linux.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: no update expected.
- Specs: update cross-platform behavior.
- End-user/operator docs: update Node.js docs.
- End-user/operator skills: no update expected.
- SOW lifecycle: child of SOW-0063.
- SOW-status.md: list as pending.

Open-source reference evidence:

- Official Node.js v22 documentation checked for `process.kill(pid, 0)`
  liveness probing, `fs.openSync()` with `O_CREAT | O_EXCL`, and platform
  identifiers such as `os.platform()`.
- `logdna/exclusive-lock-node @ 62ee91f9c7ca` was checked as a lock reference.
  `README.md:25-36` states it uses Redis/KeyDB records and does not provide
  strict exclusion for exact-once non-idempotent append work, so it was not used
  as the journal writer lock model.
- Baseline journal format evidence remains `systemd/systemd` v260.1 from project
  specs.

Open decisions:

- None. User approved parallel worktree execution.

## Implications And Decisions

1. 2026-05-30: This SOW is assigned to an isolated worktree. It should not edit
   other language implementations except shared specs/docs/tests required by the
   Node.js portability contract.

## Plan

1. Isolate Node.js platform code.
2. Implement portable lock, boot, and process helpers.
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

- Created as Node.js-only child SOW under SOW-0063 for parallel worktree
  execution.
- Confirmed user-authorized parallel implementation routing; AGENTS.md
  external-implementer exception applies for this worktree.
- Moved SOW from `pending/` to `current/` and changed status to
  `in-progress`.
- Added `node/src/lib/platform.js` for Node.js platform helpers. Linux `/proc`
  boot/process-start reads are isolated there; non-Linux lock stale-owner
  checks use Node's portable process-liveness probe and conservatively preserve
  held locks when owner identity cannot be proven stale.
- Routed direct writer and directory writer boot-ID loading through the platform
  helper. Non-Linux auto identity falls back to random UUIDs unless callers pass
  explicit `bootId` / `machineId` or use strict identity mode.
- Updated Node.js tests, Node.js README, and product scope spec for the platform
  behavior.
- Did not edit `SOW-status.md`; prompt requires status reconciliation to be left
  to the orchestrator.

## Validation

Acceptance criteria evidence:

- Node.js tests pass on Linux for affected paths:
  `npm_config_cache=../.local/npm-cache npm test` from `node/` returned
  `PASS node package tests (tests/conformance/manifests/conformance-v01.json)`.
- Node.js import/core runtime check on Linux passed:
  `node -e "import('./node/src/index.js')..."` returned
  `{"platform":"linux","arch":"x64","import_ok":true,"native_addons_loaded":[]}`.
- Writer locking preserves one-writer behavior in the shared matrix:
  `python3 tests/interoperability/run_lock_matrix.py` returned
  `total: 8, passed: 8, failed: 0`; results:
  `.local/interoperability/lock-matrix-results-20260530-083929.json`.
- Boot ID and process-owner behavior no longer assumes `/proc` at call sites:
  `rg -n "(/proc/sys/kernel/random/boot_id|/proc/\\$\\{|/proc/)" node/src -S`
  finds only `node/src/lib/platform.js`.
- Native mmap/systemd/libjournal runtime scan:
  `rg -n "(node-gyp-build|node-addon-api|ffi-napi|bindings|\\.node|mmap|libjournal|libsystemd)" node/src node/package.json -S`
  finds only libsystemd-compatible facade comments, not runtime imports.
- Specs/docs updated:
  `.agents/sow/specs/product-scope.md` and `node/README.md`.

Tests or equivalent validation:

- `npm ci --cache ../.local/npm-cache` from `node/`: passed, added four
  package dependencies under ignored `node/node_modules`.
- `npm_config_cache=../.local/npm-cache npm test` from `node/`: passed.
- Static `/proc` isolation scan: passed; only platform helper contains Linux
  `/proc` paths.
- Static native/runtime dependency scan: passed for `node/src` and
  `node/package.json`; `node/package-lock.json` still records
  `node-liblzma` native build metadata through `node-addon-api` and
  `node-gyp-build`, but the runtime source imports `node-liblzma/wasm/*` and
  the Node package test plus import check confirmed no `.node` addon was loaded.
- `git diff --check`: passed after SOW evidence update.
- `python3 tests/interoperability/run_lock_matrix.py --entries 3 --delay-ms 5`:
  failed with false contention failures because the shortened holder window let
  holders close before slower contenders ran. This shortened run is not valid
  evidence for the lock contract.
- `python3 tests/interoperability/run_lock_matrix.py`: passed with default
  holder window.
- `python3 tests/interoperability/run_matrix.py --writers node --readers node stock --entries 10`:
  passed 11/11 checks; results:
  `.local/interoperability/matrix-results-20260530-083955.json`.
- `python3 tests/interoperability/run_directory_matrix.py --readers node stock`:
  passed, including Node and stock `journalctl --directory` JSON, export, text,
  fields, boot listing, corrupt-directory skip, and Node `.journal.zst`
  extension checks.
- `.agents/sow/audit.sh`: passed with verdict
  `SOW initialization complete and clean`.

Real-use evidence:

- Stock systemd oracle available on this Linux host:
  `journalctl --version` reported `systemd 260 (260.1-2-manjaro)`.
- Linux runtime available:
  `node --version` reported `v26.1.0`; `npm --version` reported `11.14.1`.
- Linux host check:
  `uname -a` reported Linux `7.0.9-1-MANJARO` on `x86_64`.
- FreeBSD, macOS, and Windows Node runtime execution was not available inside
  this worktree. `wine` and `qemu-system-x86_64` are installed, but no
  repository-local Windows/FreeBSD/macOS Node runtime or VM image was available;
  no `node.exe` was found. Non-Linux coverage in this SOW is therefore by
  source construction, injected platform-helper tests, and documented blockers.

Reviewer findings:

- Not run in this implementation worktree. The assigned prompt forbids external
  assistants/reviewers; whole-SOW read-only review is left to the orchestrator.

Same-failure scan:

- Direct Linux boot-ID/process `/proc` reads were removed from
  `node/src/lib/lock.js`, `node/src/lib/writer.js`, and
  `node/src/lib/directory-writer.js`.
- `rg -n "(/proc/sys/kernel/random/boot_id|/proc/\\$\\{|/proc/)" node/src -S`
  now finds only `node/src/lib/platform.js`.
- Targeted scan for direct `readFileSync('/proc` and template-literal `/proc`
  reads in `node/src/lib/lock.js`, `node/src/lib/writer.js`, and
  `node/src/lib/directory-writer.js` returns no matches.

Sensitive data gate:

- Passed by inspection. Work used synthetic fixtures and repository-local
  `.local/` artifacts only; no live host journal was probed and no raw sensitive
  data was written to durable artifacts.

Artifact maintenance gate:

- `AGENTS.md`: not updated; repository workflow and guardrails did not change.
- Runtime project skills: not updated; this SOW did not change HOW future agents
  must work. The shortened lock-matrix false failure is recorded here instead of
  changing project-wide skills from an implementation worktree.
- Specs: updated `.agents/sow/specs/product-scope.md` for Node.js platform
  identity and conservative non-Linux lock stale-owner behavior.
- End-user/operator docs: updated `node/README.md` with Node.js platform
  behavior, identity, and lock semantics.
- End-user/operator skills: none exist for this project, so none were affected.
- SOW lifecycle: moved from `pending/open` to `current/in-progress`; left
  sub-state `implemented; ready for orchestrator review`.
- `SOW-status.md`: intentionally not updated, per assigned prompt; orchestrator
  must reconcile status after merge/review.

Lessons extracted:

- `run_lock_matrix.py` should be run with its default holder window for
  contention evidence. Very short entry/delay settings can create false
  failures because holders may close before slower contenders attempt to open.

Follow-up mapping:

- Parent umbrella: `SOW-0063-20260530-cross-platform-portability.md`.
- Non-Linux runtime execution evidence remains for the parent/orchestrator to
  collect with real FreeBSD, macOS, and Windows Node runtimes.
