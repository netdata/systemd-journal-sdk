# SOW-0015 - Deterministic Ingesters

## Status

Status: open

Sub-state: pending after SOW-0014 dataset completion.

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

Unknowns:

- The exact isolated build mechanism for the systemd C helper must be proven during implementation without modifying the external systemd checkout.

### Acceptance Criteria

- A systemd C ingester consumes the accepted dataset and writes a journal file using systemd internal writer APIs from the baseline source.
- Rust, Go, Node.js, and Python ingesters consume the same dataset and write journal files through their SDK writer APIs.
- All ingesters consume the rejection corpus and produce deterministic expected errors without writing rejected rows to accepted journal files.
- All ingesters support deterministic metadata from SOW-0014, including IDs, timestamps, sequence numbers, boot IDs, and compression configuration required by later byte-comparison tests.
- Build and runtime outputs are kept inside this repository, preferably under `.local/`.
- The external systemd checkout is read-only; no command writes, formats, resets, checks out, configures, or builds inside it.
- No SDK ingester links to system journal libraries.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/specs/product-scope.md`
- `.agents/sow/pending/SOW-0014-20260524-deterministic-ingestion-dataset.md`
- `systemd/systemd @ cf3156842209f8318753861a9dd2d821674f3f59`
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
- If SDK writer APIs cannot accept deterministic metadata, later byte-for-byte comparison will expose API gaps.

## Pre-Implementation Gate

Status: blocked

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

1. Wait for SOW-0014 to freeze dataset schema and corpora.
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
- Runtime project skills: update only if isolated systemd-helper build steps become mandatory future workflow.
- Specs: update if deterministic writer API options become public behavior.
- End-user/operator docs: no update expected.
- End-user/operator skills: no update expected.
- SOW lifecycle: blocked until SOW-0014 completes.
- SOW-status.md: update when created, activated, or closed.

Open-source reference evidence:

- `systemd/systemd @ cf3156842209f8318753861a9dd2d821674f3f59`
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

Pending activation.

## Validation

Pending activation and implementation.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.
