# SOW-0069 - Python Cross Platform Portability

## Status

Status: in-progress

Sub-state: implemented; ready for orchestrator review; child of SOW-0063.

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
- `python/journal/directory_writer.py`
- `python/test_all.py`
- `.agents/sow/pending/SOW-0063-20260530-cross-platform-portability.md`
- `.agents/sow/specs/product-scope.md`
- `.agents/skills/project-journal-compatibility/SKILL.md`
- Python official documentation:
  `https://docs.python.org/3.14/library/fcntl.html`,
  `https://docs.python.org/3.14/library/msvcrt.html`, and
  `https://docs.python.org/3.14/library/os.html#os.fsync`.

Current state:

- Python writer imports POSIX-only `fcntl` at module import.
- Python writer uses POSIX directory-open and advisory lock APIs.
- Python stale-lock owner detection reads Linux `/proc`.
- Python writer used Unix-only `os.pread` / `os.pwrite` in fallback paths.

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
- SOW-status.md: status reconciliation left to the orchestrator per the
  assigned worktree prompt.

Open-source reference evidence:

- Baseline remains systemd/systemd v260.1 from project specs.
- `gptme/gptme @ 111e178b0624`
  `gptme/logmanager/manager.py:304` through
  `gptme/logmanager/manager.py:324` uses the same high-level pattern of
  selecting `fcntl.flock` on POSIX and `msvcrt.locking` on Windows for
  Python file locks.

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
- Confirmed user-authorized parallel implementation routing; AGENTS.md
  external-implementer exception applies for this worktree.
- Implemented `python/journal/_platform.py` to isolate platform file locks,
  positional I/O, directory sync, boot ID, and process-owner helpers.
- Removed import-time `fcntl` dependency from `python/journal/writer.py`;
  POSIX `fcntl` is imported only inside the platform lock helper when a lock
  is acquired.
- Updated Python writer fallback paths to avoid direct `os.pread` /
  `os.pwrite` use on platforms where Python does not expose those APIs.
- Added direct-writer file-arena fallback when writable mmap creation or
  resizing is unavailable.
- Updated writer archive rename flow for platforms that require closing the
  source file before rename.
- Updated Python directory writer boot ID and directory sync behavior through
  platform helpers.
- Updated `python/test_all.py` with import-safety, portable-lock,
  positional-I/O fallback, directory-sync fallback, mmap fallback, and
  closed-before-rename archive tests.
- Updated `.agents/sow/specs/product-scope.md` and `python/README.md` with
  Python platform behavior.
- Installed validation-only dependencies under repo-local ignored paths:
  `.local/python-venv`, `.local/pip-cache`, `.local/npm-cache`, and
  `node/node_modules`.

## Validation

Acceptance criteria evidence:

- Python import works on Linux:
  `PYTHONPATH="$PWD/python" PIP_CACHE_DIR="$PWD/.local/pip-cache" python -c "import journal; print('ok')"`
  printed `ok`.
- Windows import-safety is demonstrated by blocking `fcntl` imports in a
  subprocess and importing `journal`; the command printed `ok`.
- Linux affected reader/writer/facade paths pass through
  `python/test_all.py` using `.local/python-venv/bin/python`.
- Python writer one-writer behavior passed `test_writer_exclusive_lock`,
  the new portable owner fallback test, and the shared lock matrix.
- Directory writer rotation/retention paths passed `python/test_all.py` and
  the shared directory interoperability matrix.
- Specs/docs now describe the Python platform behavior in
  `.agents/sow/specs/product-scope.md` and `python/README.md`.

Tests or equivalent validation:

- PASS:
  `PYTHONPATH="$PWD/python" PIP_CACHE_DIR="$PWD/.local/pip-cache" python -c "import journal; print('ok')"`
- PASS:
  `PYTHONPATH="$PWD/python" PIP_CACHE_DIR="$PWD/.local/pip-cache" python - <<'PY' ... import blocker for fcntl ... PY`
- PASS:
  `PYTHONPATH="$PWD/python" PIP_CACHE_DIR="$PWD/.local/pip-cache" python -m compileall python`
- Initial full-package run with the system interpreter exposed missing local
  dependency `lz4`: `ModuleNotFoundError: No module named 'lz4'`.
  Resolution: created `.local/python-venv` and installed
  `python/requirements.txt` using `--cache-dir "$PWD/.local/pip-cache"`.
- PASS:
  `PYTHONPATH="$PWD/python" PIP_CACHE_DIR="$PWD/.local/pip-cache" .local/python-venv/bin/python python/test_all.py`
  printed `PASS python package tests (python/test_all.py)`.
- Initial lock matrix run exposed missing Node dependencies before lock
  checks: Node could not load `node/src/lib/lz4-block.js`.
  Resolution: `npm ci --prefix node --cache "$PWD/.local/npm-cache"`.
- PASS:
  `PIP_CACHE_DIR="$PWD/.local/pip-cache" python tests/interoperability/run_lock_matrix.py --entries 20 --delay-ms 5`
  reported `total: 8, passed: 8, failed: 0` with systemd
  `260 (260.1-2-manjaro)`.
- PASS:
  `PYTHON="$PWD/.local/python-venv/bin/python" PIP_CACHE_DIR="$PWD/.local/pip-cache" python tests/interoperability/run_directory_matrix.py`
  reported `"status": "PASS"` with stock, Go, Rust, Node.js, and Python
  readers.
- PASS: `git diff --check`.

Real-use evidence:

- Direct Linux runtime evidence used Python `3.14.5`, Node.js `v26.1.0`,
  Go `go1.26.3-X:nodwarf5`, Rust `1.91.1`, and stock systemd
  `260 (260.1-2-manjaro)`.
- Host OS evidence: `uname -a` reported Linux
  `7.0.9-1-MANJARO` on `x86_64`.
- Windows runtime blocker: `wine` exists, but under repo-local
  `WINEPREFIX="$PWD/.local/wineprefix"`, `wine cmd /c where python.exe`
  returned `File not found`; no Windows Python runtime is available locally.
- FreeBSD/macOS runtime blockers: `command -v freebsd-version` and
  `command -v sw_vers` returned no executable on this Linux workstation.

Reviewer findings:

- Not run in this worktree by instruction. Whole-SOW read-only reviewer pass is
  left to the orchestrator after merge/reconciliation.

Same-failure scan:

- `rg -n "import fcntl|fcntl\\.|os\\.pread|os\\.pwrite|O_DIRECTORY|/proc/sys/kernel/random/boot_id|/proc/\\{pid\\}|/proc/\\d" python/journal python/test_all.py`
  shows the remaining POSIX/procfs operations are isolated in
  `python/journal/_platform.py`; there is no package import-time `fcntl`
  dependency.

Sensitive data gate:

- Synthetic fixtures and generated local test journals only.
- No live host journal probing, `systemd-cat`, `logger`, `/var/log/journal`,
  or `/run/log/journal` access was used.
- Durable artifacts contain no raw credentials, customer identifiers, personal
  data, private endpoints, or production logs.

Artifact maintenance gate:

- `AGENTS.md`: not updated; no project-wide workflow change.
- Runtime project skills: not updated; no durable workflow rule changed.
- Specs: updated `.agents/sow/specs/product-scope.md` for Python platform
  behavior.
- End-user/operator docs: updated `python/README.md`.
- End-user/operator skills: no output/reference skills exist for this project.
- SOW lifecycle: moved this SOW from `pending/` to `current/`, changed
  `Status: open` to `Status: in-progress`, and left it in-progress with
  sub-state `implemented; ready for orchestrator review`.
- `SOW-status.md`: intentionally not edited per assigned worktree prompt;
  status reconciliation is left to the orchestrator.

Lessons extracted:

- Python import portability requires checking not only top-level imports but
  every fallback path used before a mapped arena exists; `os.pread` /
  `os.pwrite` were Unix-only risks adjacent to the original `fcntl` blocker.
- Non-Linux stale-lock cleanup should prefer conservative liveness checks over
  unsafe false-stale decisions when precise process start time is unavailable.

Follow-up mapping:

- Parent umbrella: `SOW-0063-20260530-cross-platform-portability.md`.
- Orchestrator review/reconciliation: this SOW remains in `current/` and is not
  marked completed by this worktree.
- Non-Linux runtime execution: exact local blockers are recorded above; parent
  SOW-0063 owns cross-OS runtime matrix reconciliation.
