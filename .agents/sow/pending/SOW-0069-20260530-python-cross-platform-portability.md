# SOW-0069 - Python Cross Platform Portability

## Status

Status: open

Sub-state: pending Python-only portability implementation; child of SOW-0063.

## Requirements

### Purpose

Make the Python SDK import, read, write, rotate, retain, and verify journal
files on Linux, FreeBSD, macOS, and Windows without changing the shared SDK API
contracts.

### User Request

The user requested SDK portability to Linux, FreeBSD, macOS, and Windows, and
approved parallel worktrees for independent SOWs.

### Assistant Understanding

Facts:

- This SOW covers Python only.
- SOW-0063 recorded that Python imports POSIX-only `fcntl` from the writer
  module.
- SOW-0063 recorded Linux `/proc` assumptions in Python stale-lock owner
  detection.
- Python must remain API-compatible with the shared SDK and facade contracts.

Inferences:

- Python must avoid import-time POSIX-only dependencies.
- Platform behavior should be behind helpers rather than scattered checks.

Unknowns:

- Whether all Python compression dependencies support every target in the
  accepted runtime policy.
- Which non-Linux runtime environments are available locally for execution.

### Acceptance Criteria

- Python import works on Linux and is demonstrably import-safe for Windows.
- Python tests pass on Linux for affected reader/writer/facade paths.
- Windows, FreeBSD, and macOS checks are added where possible or exact blockers
  are recorded.
- Python writer locking preserves one-writer behavior on supported targets.
- Python directory writer handles rotation/retention with platform-appropriate
  directory sync semantics.
- Specs/docs describe Python platform behavior.

## Analysis

Sources checked:

- `python/journal/__init__.py`
- `python/journal/writer.py`
- `python/journal/lock.py`
- `.agents/sow/pending/SOW-0063-20260530-cross-platform-portability.md`
- `.agents/skills/project-journal-compatibility/SKILL.md`

Current state:

- Python writer imports POSIX-only `fcntl` at module import.
- Python writer uses POSIX directory-open and advisory lock APIs.
- Python stale-lock owner detection reads Linux `/proc`.

Risks:

- Import-time platform failure makes even read-only use impossible on Windows.
- Weak lock fallback can allow concurrent writers.
- Platform fallbacks can drift from Rust behavior if not covered.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Python portability is blocked by POSIX imports and Linux `/proc`
  assumptions. These must be isolated while preserving existing public APIs and
  shared journal contracts.

Evidence reviewed:

- SOW-0063 import and source evidence.
- Project compatibility skill cross-language API and journal behavior
  requirements.

Affected contracts and surfaces:

- Python package import.
- Python reader, writer, directory writer, lock handling, and facade APIs.
- Compression dependency runtime policy.
- Platform docs/specs.

Existing patterns to reuse:

- Existing Python facade and writer tests.
- Existing shared conformance fixtures.
- Existing lockfile format `systemd-journal-sdk-lock-v1`.

Risk and blast radius:

- Medium. Python is not the critical Netdata hot writer path, but correctness
  and parity are required.

Sensitive data handling plan:

- Use synthetic fixtures only; do not read host live journals or record raw log
  payloads.

Implementation plan:

1. Move POSIX-only imports behind platform helpers.
2. Add portable lock and process-owner behavior.
3. Add directory sync and mmap/read fallback behavior where needed.
4. Run tests/checks and update docs/specs.

Validation plan:

- Linux Python tests for affected paths.
- Windows import simulation or runtime check proving no `fcntl` requirement.
- Platform checks where available.
- Relevant shared conformance/interoperability tests on Linux.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: no update expected.
- Specs: update cross-platform behavior.
- End-user/operator docs: update Python docs.
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
   Python portability contract.

## Plan

1. Isolate Python platform code.
2. Implement portable import, lock, and directory helpers.
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

- Created as Python-only child SOW under SOW-0063 for parallel worktree
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
