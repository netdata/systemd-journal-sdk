# SOW-0014 - Deterministic Ingestion Dataset

## Status

Status: in-progress

Sub-state: active dataset design before deterministic ingester work.

## Requirements

### Purpose

Create a deterministic ingestion dataset that proves writer correctness and supports performance profiling for the Rust, Go, Node.js, Python, and systemd reference writers.

### User Request

The user requested a dataset that exercises all or most writer paths, including edge cases such as empty fields, too-long keys, too-long values, binary values, mixed field cardinality, and a large performance corpus of about 200k rows.

### Assistant Understanding

Facts:

- The same dataset must drive systemd reference ingestion and all SDK ingesters.
- The dataset quality determines whether byte-level writer comparison and performance profiling are meaningful.
- Invalid input cases cannot be part of the byte-identical output corpus because systemd rejects invalid journal fields before writing.

Inferences:

- The dataset must be split into accepted records, rejection cases, and large accepted performance records.
- Accepted records must be deterministic down to IDs, timestamps, boot IDs, field ordering, binary payload bytes, and rotation boundaries.
- Rejection cases must verify error parity without writing those rows to the byte-comparison journals.

Unknowns:

- Exact corpus format and generator implementation will be selected during this SOW from repository patterns and reviewer feedback.

### Acceptance Criteria

- A committed dataset schema describes accepted records, rejected records, performance records, deterministic IDs, timestamps, and expected outcomes.
- A committed deterministic generator produces the accepted correctness corpus and the large performance corpus.
- The correctness corpus covers writer paths for new FIELD objects, reused FIELD objects, new DATA objects, reused DATA objects, duplicate fields in one entry, duplicate values across entries, binary field values, embedded NUL bytes in values, zero-length values after `=`, large values, values near compression thresholds, high-cardinality fields, low-cardinality fields, hash collision pressure where practical, entry-array growth, data-entry-array growth, and sorted entry-item ordering.
- The rejection corpus covers empty field names, lowercase field names, field names beginning with digits, field names longer than 64 bytes, field names with invalid characters, missing `=`, empty full data payloads, null field payloads where adapters can express them, and values that exceed accepted implementation limits.
- The large performance corpus contains about 200k accepted rows and includes a documented mixed-cardinality profile centered around about 32 fields per row.
- Dataset generation is deterministic across repeated runs on the same repository commit.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/specs/product-scope.md`
- `.agents/sow/current/SOW-0008-20260523-interoperability-and-full-writer-features.md`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
- `src/libsystemd/sd-journal/journal-file.c:1710`
- `src/libsystemd/sd-journal/journal-file.c:1844`

Current state:

- Existing interoperability tests generate useful simple synthetic entries, but they do not yet define a frozen corpus intended for byte-level writer equivalence and performance profiling.
- systemd validates field names and DATA payloads before appending, so invalid edge cases must be represented as expected rejections rather than accepted journal entries.

Risks:

- A weak dataset can make byte-for-byte equality meaningless.
- A dataset that mixes rejected cases with accepted journal rows can create false failures and hide writer bugs.
- A performance dataset with unrealistic cardinality can optimize the wrong hot paths.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Writer correctness cannot be proven by simple smoke entries. The project needs a deterministic corpus that stresses object reuse, indexing, binary field payloads, ordering, cardinality, and rejection behavior so every writer ingests the same semantic input and produces comparable files or comparable errors.

Evidence reviewed:

- Product scope requires cross-language writer/readers and live concurrency compatibility.
- Existing matrix tests under `tests/interoperability/` use generated entries but do not freeze a full edge-case dataset.
- systemd validates field names in `journal_field_valid()` and rejects invalid DATA payloads in `journal_file_append_data()`.

Affected contracts and surfaces:

- Shared conformance fixtures.
- Writer ingestion tools.
- Byte-level comparison harness.
- Benchmark and profiling harness.
- Documentation of dataset semantics.

Existing patterns to reuse:

- `tests/interoperability/` matrix layout.
- `tests/conformance/` shared fixture structure.
- Per-language livewriter commands.
- Project-local `.local/` for generated outputs.

Risk and blast radius:

- Medium. This SOW adds test data and generators, not SDK writer behavior, but the dataset becomes a long-lived correctness contract for all later writer work.

Sensitive data handling plan:

- Dataset rows must be synthetic. Durable artifacts must not contain secrets, credentials, bearer tokens, SNMP communities, customer names, personal data, non-private customer-identifying IPs, private endpoints, or proprietary incident details.

Implementation plan:

1. Define dataset schema, deterministic metadata, accepted corpus, rejection corpus, and performance corpus.
2. Implement deterministic generator and committed metadata.
3. Add lightweight validation that regeneration is stable and rejection expectations are machine-readable.
4. Document dataset coverage and limitations.

Validation plan:

- Regenerate dataset twice and verify identical outputs.
- Validate schema and expected outcome metadata.
- Run dataset validation without invoking SDK writers yet.
- Run project-local SOW audit.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: update only if dataset workflow becomes mandatory for future writer work.
- Specs: update product scope only if the dataset changes public correctness guarantees.
- End-user/operator docs: no update expected.
- End-user/operator skills: no update expected.
- SOW lifecycle: remains pending until active; closes only with validation evidence.
- SOW-status.md: update when created, activated, or closed.

Open-source reference evidence:

- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
- `src/libsystemd/sd-journal/journal-file.c:1710`
- `src/libsystemd/sd-journal/journal-file.c:1844`

Open decisions:

- None blocking. Dataset format is an implementation detail as long as it is deterministic, language-neutral, and reviewable.

## Implications And Decisions

1. Dataset partitioning
   - Decision: split data into accepted correctness records, rejected records, and accepted performance records.
   - Reason: invalid journal fields cannot be written by systemd, so they cannot appear in byte-identical journal output.

## Plan

1. Build dataset schema and generator.
2. Validate deterministic regeneration.
3. Document coverage and known gaps.
4. Prepare SOW-0015 to consume the frozen dataset.

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

- Activated after SOW-0008 closeout commit `dc4f892`.
- Verified the baseline tag without changing the external checkout: `git ls-remote https://github.com/systemd/systemd.git 'refs/tags/v260.1*'` reports `refs/tags/v260.1^{}` as `c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`.
- Verified source-reference lines from GitHub raw content for the baseline commit: `journal_field_valid()` at `src/libsystemd/sd-journal/journal-file.c:1710` and `journal_file_append_data()` at `src/libsystemd/sd-journal/journal-file.c:1844`.

## Validation

Activation evidence:

- Passed: SOW-0008 closeout commit `dc4f892` exists before activation.
- Passed: baseline systemd tag evidence confirmed with read-only `git ls-remote`.
- Pending: dataset implementation, review, deterministic regeneration checks, and final SOW audit.

Sensitive data gate:

- Activation edits introduce only synthetic dataset planning text and upstream source references. No secrets, credentials, bearer tokens, SNMP communities, customer names, personal data, non-private customer-identifying IPs, private endpoints, or proprietary incident details are present.

Artifact maintenance gate:

- AGENTS.md: no update needed for activation.
- Runtime project skills: no update needed for activation.
- Specs: no shipped product behavior changed during activation.
- End-user/operator docs: no update needed for activation.
- End-user/operator skills: no output/reference skill is produced during activation.
- SOW lifecycle: moved from `pending/` to `current/` with `Status: in-progress`.
- SOW-status.md: updated for SOW-0014 activation.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.
