# SOW-0070 - Node Cross Platform Portability

## Status

Status: open

Sub-state: pending Node.js-only portability implementation; child of SOW-0063.

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

- None added; baseline remains systemd/systemd v260.1 from project specs.

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

## Validation

Acceptance criteria evidence:

- Pending.

Tests or equivalent validation:

- Pending.

Real-use evidence:

- Pending.

Reviewer findings:

- Pending.

Same-failure scan:

- Pending.

Sensitive data gate:

- Pending.

Artifact maintenance gate:

- Pending.

Lessons extracted:

- Pending.

Follow-up mapping:

- Parent umbrella: `SOW-0063-20260530-cross-platform-portability.md`.
