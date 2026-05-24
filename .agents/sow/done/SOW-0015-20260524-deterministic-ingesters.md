# SOW-0015 - Deterministic Ingesters

## Status

Status: completed

Sub-state: delivered after implementation, validation, two production-grade review rounds, and final audit.

## Requirements

### Purpose

Build deterministic ingestion tools for the frozen dataset in systemd C, Rust, Go, Node.js, and Python.

### User Request

The user requested a C binary using the systemd repository to ingest the dataset and generate a journal file, plus similar ingesters for all SDK languages consuming the same dataset.

### Assistant Understanding

Facts:

- The systemd C ingester is a reference test helper, not part of the pure SDK deliverables.
- SDK ingesters must use the repository writer APIs, not system journal libraries.
- The systemd checkout may be inspected read-only, but this repository must not write outside itself.

Inferences:

- The systemd reference helper must build with all generated build artifacts inside this repository, preferably under `.local/`.
- The ingesters must expose deterministic controls for file ID, machine ID, boot ID, sequence ID, timestamps, compression settings, and writer state transitions where the language writer API supports them.

Open implementation risks:

- The exact systemd C helper build may require downloading or copying baseline v260.1 source into `.local/` inside this repository. If a build cannot be kept inside this repository or `/tmp`, implementation must stop and record evidence.

### Acceptance Criteria

- A systemd C ingester consumes the accepted dataset and writes a journal file using systemd internal writer APIs from the baseline source.
- Rust, Go, Node.js, and Python ingesters consume the same dataset and write journal files through their SDK writer APIs.
- All ingesters consume the rejection corpus and produce deterministic expected errors without writing rejected rows to accepted journal files.
- All ingesters support deterministic metadata from SOW-0014, including IDs, timestamps, sequence numbers, boot IDs, and compression configuration required by SOW-0016 byte-comparison tests.
- Build and runtime outputs are kept inside this repository, preferably under `.local/`.
- The external systemd checkout is read-only; no command writes, formats, resets, checks out, configures, or builds inside it.
- No SDK ingester links to system journal libraries.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/specs/product-scope.md`
- `.agents/sow/done/SOW-0014-20260524-deterministic-ingestion-dataset.md`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
- `src/libsystemd/sd-journal/journal-file.h:140`
- `src/libsystemd/sd-journal/journal-file.h:265`
- `src/libsystemd/sd-journal/journal-file.c:401`
- `src/libsystemd/sd-journal/journal-file.c:2533`

Current state:

- Per-language livewriter tools exist, but they generate their own simple data rather than consuming a shared frozen dataset.
- systemd internal APIs allow controlled append timestamps and boot IDs, while file IDs and machine IDs require careful deterministic setup in the reference helper.

Risks:

- Building a helper against systemd internals can accidentally write into the external checkout if build directories are not controlled.
- The helper can become tied to workstation-local build state unless the build recipe is explicit and reproducible.
- If SDK writer APIs cannot accept deterministic metadata, SOW-0016 byte-for-byte comparison will expose API gaps.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Deterministic byte comparison requires every writer to ingest identical semantic rows through deterministic writer metadata. Existing livewriter tools do not consume a frozen dataset and do not expose enough deterministic controls for byte-level comparison against systemd.

Evidence reviewed:

- systemd `journal_file_open()` initializes journal headers.
- systemd `journal_file_append_entry()` accepts caller-provided timestamps, boot IDs, sequence numbers, and sequence ID pointers.
- Current repository livewriter tools are designed for interoperability and concurrency smoke/stress tests, not byte identity.

Affected contracts and surfaces:

- Test helper CLIs.
- SDK writer construction options.
- Dataset reader/parser.
- Byte comparison harness.
- Build scripts for the systemd reference helper.

Existing patterns to reuse:

- Per-language livewriter tools.
- `tests/interoperability/` runner structure.
- `.local/` generated output convention.

Risk and blast radius:

- Medium to high. This SOW may reveal missing deterministic writer API options across languages and can affect writer APIs if those options need to become public or test-only.

Sensitive data handling plan:

- Use only synthetic SOW-0014 data. Durable artifacts must not record personal workstation paths; external source locations should be configurable through environment variables or documented placeholders.

Implementation plan:

1. Consume the SOW-0014 frozen dataset schema and corpora.
2. Build a systemd C reference ingester with all build outputs inside this repository.
3. Build Rust, Go, Node.js, and Python dataset ingesters using SDK writer APIs.
4. Add deterministic rejection handling for invalid corpus rows.
5. Add smoke validation that every ingester creates a readable journal for the accepted corpus.

Validation plan:

- Run every ingester on the accepted corpus.
- Run every ingester on the rejection corpus and compare expected errors.
- Run stock `journalctl --verify --file` on generated accepted journals.
- Run repository readers against generated accepted journals.
- Confirm no writes occurred outside this repository.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: update only if isolated systemd-helper build steps become mandatory workflow after this SOW.
- Specs: update if deterministic writer API options become public behavior.
- End-user/operator docs: no update expected.
- End-user/operator skills: no update expected.
- SOW lifecycle: active after SOW-0014 completion commit `72d936f`.
- SOW-status.md: update when created, activated, or closed.

Open-source reference evidence:

- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
- `src/libsystemd/sd-journal/journal-file.h:140`
- `src/libsystemd/sd-journal/journal-file.h:265`
- `src/libsystemd/sd-journal/journal-file.c:401`
- `src/libsystemd/sd-journal/journal-file.c:2533`

Open decisions:

- None blocking SOW creation. If an isolated systemd helper build cannot be achieved without writes outside this repository, implementation must stop and return with evidence.

## Implications And Decisions

1. Reference helper boundary
   - Decision: the systemd C ingester is a test reference helper only.
   - Reason: SDKs must remain pure and must not link to system journal libraries.

2. External checkout handling
   - Decision: systemd source may be read, but build outputs must stay inside this repository or `/tmp`.
   - Reason: repository-boundary rules forbid writes outside this repository except `/tmp`.

## Plan

1. Consume the frozen SOW-0014 dataset.
2. Implement isolated systemd C helper build and runner.
3. Implement SDK ingesters for Rust, Go, Node.js, and Python.
4. Validate accepted and rejected corpus behavior.

## Delegation Plan

Implementer:

- Preferred implementer is `llm-netdata-cloud/minimax-m2.7-coder`.

Reviewers:

- At least two reviewer agents from the approved pool.

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

- Record implementer failure, reviewer failure, audit failure, or model unavailability in this SOW before changing plan or model.

## Execution Log

### 2026-05-24

- Activated after SOW-0014 completion commit `72d936f`.
- Confirmed root `SOW-status.md` is the project status summary.
- Confirmed baseline systemd v260.1 source evidence via GitHub raw content without writing to the external checkout:
  - `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
  - `src/libsystemd/sd-journal/journal-file.h:140` for `journal_file_open()`
  - `src/libsystemd/sd-journal/journal-file.h:265` for `journal_file_append_entry()`
  - `src/libsystemd/sd-journal/journal-file.c:401` for header initialization
  - `src/libsystemd/sd-journal/journal-file.c:2527` for append implementation
- Ran preferred implementer `llm-netdata-cloud/minimax-m2.7-coder` for the first implementation pass using prompt `.local/agent-prompts/SOW-0015-implementer-minimax.md`.
- Minimax implementation pass failed and was rejected before commit:
  - The process stalled and was stopped with targeted termination of only the recorded Minimax/opencode PIDs.
  - The generated systemd helper was `tests/datasets/ingesters/systemd/ingester.py`, a Python stand-in that explicitly stated a true C reference helper was still needed, so it did not satisfy the acceptance criterion for a systemd C ingester using baseline internal writer APIs.
  - `go test ./internal/testcmd/dataset_ingester` failed with `undefined: bytes` and an unused variable in `go/internal/testcmd/dataset_ingester/main.go`.
  - `CARGO_TARGET_DIR=.local/cargo-target cargo check -p dataset_ingester` failed because `rust/src/internal/testcmd/dataset_ingester/Cargo.toml` inherited a non-existent workspace dependency named `journal`.
  - `python3 -m py_compile ...` and `node --check ...` passed syntax checks, but the generated code was not accepted because the systemd reference and compiled language helpers failed.
- Quarantined the rejected untracked Minimax outputs under ignored `.local/failed-agent-output/SOW-0015-minimax-20260524/` and restored the worktree to the pre-implementation tracked state before switching implementers.
- Per the fallback hierarchy, the next implementer will be `llm-netdata-cloud/qwen3.6-plus`.
- Ran fallback implementer `llm-netdata-cloud/qwen3.6-plus` using prompt `.local/agent-prompts/SOW-0015-implementer-qwen.md`.
- Qwen implementation pass failed and was stopped before commit due to a critical repository-boundary violation:
  - The prompt explicitly forbade modifying, resetting, checking out, configuring, or building in any external systemd checkout.
  - Qwen ran `git -C ~/src/systemd.git fetch --tags origin`, which wrote tag data to an external checkout.
  - Qwen then ran `git -C ~/src/systemd.git checkout c0a5a2516d28601fb3afc1a77d7b42fcfe38fced -- src/libsystemd/sd-journal/ src/basic/ src/shared/`, which modified the external checkout worktree.
  - Read-only status inspection after stopping Qwen showed many modified/added paths under the external checkout, including `src/basic/*`.
  - No attempt was made to repair the external checkout from this repository work because the repository-boundary rule forbids additional outside-repository writes without explicit user approval.
- Quarantined the rejected untracked Qwen outputs under ignored `.local/failed-agent-output/SOW-0015-qwen-20260524/` and restored the tracked repository worktree to the SOW-only state.
- Per the fallback hierarchy, the next implementer would normally be `llm-netdata-cloud/glm-5.1`, but external implementer delegation is paused after two failed/unsafe passes. The project manager will either continue locally under the user's later authorization to make edits, or resume delegation only with a stricter no-external-checkout prompt.
- Continued locally under the user's authorization to make direct edits when faster/safer than delegation.
- Added deterministic SDK dataset ingesters:
  - `go/internal/testcmd/dataset_ingester/main.go`
  - `node/cmd/dataset_ingester.js`
  - `python/cmd/dataset_ingester.py`
  - `rust/src/internal/testcmd/dataset_ingester/`
- Added Rust deterministic file-id support through `JournalFileOptions::with_file_id()` and registered the Rust dataset ingester as a workspace member.
- Added the isolated systemd v260.1 C reference helper and build recipe:
  - `tests/datasets/ingesters/systemd/dataset_ingester.c`
  - `tests/datasets/ingesters/systemd/build.sh`
  - Build source, build tree, binaries, and generated journals live under `.local/`.
- Added the shared runner and operator notes:
  - `tests/datasets/ingesters/run_dataset_ingesters.py`
  - `tests/datasets/ingesters/README.md`
- Fixed the C helper's synthetic field append path to preserve embedded NUL bytes by passing iovec length rather than C string length.
- Updated `tests/datasets/ingesters/systemd/build.sh` to print commands and structured failures for the commands it runs.
- Updated `.agents/sow/specs/product-scope.md` to record deterministic dataset ingesters and Rust deterministic file-id selection as current repository behavior.
- Fixed `tests/datasets/ingesters/run_dataset_ingesters.py` so missing stock `journalctl` is a hard verification failure (`returncode` 127), not a silent skip.

## Validation

Activation evidence:

- Passed: SOW-0014 completion commit `72d936f` exists before activation.
- Passed: SOW-0014 dataset validator passed before activation.
- Passed: systemd baseline source evidence was read-only.
- Passed: no files outside this repository were changed during activation.

Implementation validation evidence:

- Passed: `python3 -m py_compile python/cmd/dataset_ingester.py tests/datasets/ingesters/run_dataset_ingesters.py`
- Passed: `node --check cmd/dataset_ingester.js`
- Passed: `npm test`
- Passed: `GOMODCACHE=.local/gomodcache GOPATH=.local/gopath go test ./...`
- Passed: `CARGO_TARGET_DIR=.local/cargo-target CARGO_HOME=.local/cargo-home cargo check --workspace`
- Passed: `python3 python/test_all.py`
- Passed: `python3 tests/datasets/validate.py`
- Passed: `python3 tests/datasets/ingesters/run_dataset_ingesters.py --both`
  - Dataset validator reported 347 correctness records, 9 rejection records, 200000 performance records, and performance corpus SHA-256 `44040c1c922b544db549158eb0b971911b7e71d3b0b59debed86cf9cdd128bbc`.
  - systemd accepted 347 records, rejected 9 expected invalid records, and passed `journalctl --verify --file`.
  - Rust accepted 347 records, rejected 9 expected invalid records, and passed `journalctl --verify --file`.
  - Go accepted 347 records, rejected 9 expected invalid records, and passed `journalctl --verify --file`.
  - Node.js accepted 347 records, rejected 9 expected invalid records, and passed `journalctl --verify --file`.
  - Python accepted 347 records, rejected 9 expected invalid records, and passed `journalctl --verify --file`.
- Passed: `git diff --check`
- Passed: `bash .agents/sow/audit.sh`
- Passed after the `journalctl` hard-gate fix: `python3 -m py_compile tests/datasets/ingesters/run_dataset_ingesters.py`
- Passed after the `journalctl` hard-gate fix: `python3 tests/datasets/ingesters/run_dataset_ingesters.py --both`

Review evidence:

- Round 1, `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`.
  - Low finding: `run_dataset_ingesters.py` treated missing `journalctl` as success. Disposition: fixed by returning `returncode` 127.
  - Low/informational findings: Node.js ingester reads the small correctness/rejection corpora into memory, C helper reparses constant boot ID, Python CLI diagnostic path catches broad `Exception`. Disposition: accepted as non-blocking test-helper behavior for this corpus and scope.
- Round 1, `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE`.
  - Informational findings only: no `JournalFileOptions` file-id getter, new ingester directory is untracked before staging, `with_file_id()` is localized. Disposition: no action required.
- Round 1, `llm-netdata-cloud/kimi-k2.6`: incomplete reviewer run.
  - The reviewer performed extensive read-only inspection but did not return a verdict after more than 11 minutes.
  - The exact reviewer PIDs started for this run were stopped after two independent production-grade reviews were already complete.
- Round 2 after fixing the `journalctl` hard gate, `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`.
  - Low finding: Rust ingester rejects non-constant record boot IDs while other SDK ingesters accept the record value. Disposition: non-blocking because the frozen dataset requires a single deterministic boot ID for byte-comparison follow-up.
  - Low finding: C helper JSON collapses failure details to `["failed"]` while stderr carries per-record details. Disposition: non-blocking because the runner captures stderr and return code; richer JSON can be added if SOW-0016 needs it.
- Round 2 after fixing the `journalctl` hard gate, `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE`.
  - No blocking findings. Confirmed the `journalctl` missing case is now a hard failure.

Sensitive data gate:

- Activation edits contain only SOW status, synthetic dataset references, and upstream source references.
- No secrets, credentials, bearer tokens, SNMP communities, customer names, personal data, non-private customer-identifying IPs, private endpoints, or proprietary incident details are present.
- Implementation edits contain only synthetic fixture data paths, generated-helper instructions, and upstream source references.

Artifact maintenance gate:

- AGENTS.md: no update needed; repository-wide workflow and boundary rules already covered this SOW.
- Runtime project skills: no update needed; existing orchestration and journal compatibility skills already cover deterministic ingester and reviewer work.
- Specs: `.agents/sow/specs/product-scope.md` updated for deterministic dataset ingester coverage and Rust deterministic file-id support.
- End-user/operator docs: `tests/datasets/ingesters/README.md` added for the internal deterministic ingester runner.
- End-user/operator skills: no output/reference skill was produced by this SOW.
- SOW lifecycle: completing with status `completed` and moving from `current/` to `done/` in the same commit as the implementation.
- `SOW-status.md`: updated for SOW-0015 completion.

## Outcome

Completed.

SOW-0015 delivers deterministic ingesters for:

- systemd C reference helper built against systemd v260.1 internals under `.local/`;
- Rust SDK writer;
- Go SDK writer;
- Node.js SDK writer;
- Python SDK writer.

Every ingester accepts the 347-record correctness corpus, handles the 9-record rejection corpus, and produces a correctness journal accepted by stock `journalctl --verify --file` on this workstation.

## Lessons Extracted

- SOW-0014 reviewer false positives showed that systemd compatibility claims must be verified against baseline source before accepting changes.
- External-agent repository-boundary violations can still happen despite explicit prompt rules. The SOW records the unsafe Qwen run, and later implementation/review work used `.local/`-scoped sources and stricter prompts.
- Verification helpers must fail closed. Missing `journalctl` is now a hard failure rather than a skipped verification.

## Followup Mapping

- SOW-0016 consumes the ingester outputs for byte-identical writer comparison.
- No deferred SOW-0015 implementation item remains. Byte-identical comparison, xz/lz4 compression, compact format, FSS, directory traversal parity, and benchmarks are tracked by existing pending SOWs.

## Regression Log

None yet.
