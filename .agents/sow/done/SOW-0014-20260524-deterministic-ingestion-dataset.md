# SOW-0014 - Deterministic Ingestion Dataset

## Status

Status: completed

Sub-state: deterministic dataset implemented, validated, reviewed, and ready for SOW-0015 ingester consumption.

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

Resolved items:

- Corpus format, generator implementation, validation, and reviewer findings are closed in this SOW.

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
- systemd validates field names and DATA payloads before writing, so invalid edge cases must be represented as expected rejections rather than accepted journal entries.

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

- Medium. This SOW adds test data and generators, not SDK writer behavior, but the dataset becomes a long-lived correctness contract for writer work after this SOW.

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
- Runtime project skills: update only if dataset workflow becomes mandatory for writer work after this SOW.
- Specs: update product scope only if the dataset changes public correctness guarantees.
- End-user/operator docs: no update expected.
- End-user/operator skills: no update expected.
- SOW lifecycle: activated from the open-work directory, closed by moving to `done/` with validation evidence.
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
- Preferred implementer `llm-netdata-cloud/minimax-m2.7-coder` was run with the repository-boundary prompt, but exited after creating only a partial `tests/datasets/schema.schema.json`. It did not add a generator, corpora, validator, docs, or SOW evidence, so the attempt is not accepted as implementation completion.
- Per the user's instruction allowing direct edits when faster, local implementation completed the dataset package and Minimax is switched to reviewer for this SOW.
- Added dataset artifacts:
  - `tests/datasets/schema.schema.json`
  - `tests/datasets/generate.py`
  - `tests/datasets/validate.py`
  - `tests/datasets/README.md`
  - `tests/datasets/ingestion-manifest.json`
  - `tests/datasets/correctness/corpus.jsonl`
  - `tests/datasets/rejections/corpus.jsonl`
  - `tests/datasets/performance/manifest.json`
- Dataset shape:
  - accepted correctness records: 347;
  - rejection cases: 9;
  - performance stream records: 200000;
  - performance stream fields per row: exactly 32;
  - performance stream hash: `44040c1c922b544db549158eb0b971911b7e71d3b0b59debed86cf9cdd128bbc`.

## Validation

Acceptance criteria evidence:

- Passed: SOW-0008 closeout commit `dc4f892` exists before activation.
- Passed: baseline systemd tag evidence confirmed with read-only `git ls-remote`.
- Passed: dataset implementation produced schema, generator, validator, committed correctness and rejection corpora, committed performance manifest, and README.
- Passed: correctness corpus contains 347 accepted records.
- Passed: rejection corpus contains 9 expected rejection cases.
- Passed: performance stream contains 200000 records and exactly 32 fields per row.
- Passed: performance stream SHA-256 is `44040c1c922b544db549158eb0b971911b7e71d3b0b59debed86cf9cdd128bbc`.
- Passed: correctness coverage includes all 17 required tags.
- Passed: rejection coverage includes all 9 required tags.

Tests and equivalent validation:

- Passed: `python3 -m py_compile tests/datasets/generate.py tests/datasets/validate.py`.
- Passed: `python3 tests/datasets/validate.py` returned 347 correctness records, 9 rejection records, 200000 performance records, and the expected performance SHA-256 hash.
- Passed: `python3 tests/datasets/generate.py performance --output .local/datasets/performance-corpus.jsonl` materialized 200000 rows under `.local/`; `sha256sum` matched `44040c1c922b544db549158eb0b971911b7e71d3b0b59debed86cf9cdd128bbc`; file size was 644M.
- Passed: `python3 tests/datasets/generate.py performance-hash` returned the same 200000-row hash without writing the performance file.
- Passed: independent JSON Schema validation of `ingestion-manifest.json`, all 347 correctness records, and all 9 rejection records.
- Passed after final schema tightening: `schema.schema.json` requires `invocation_id` for accepted records.
- Passed after final schema tightening: `schema.schema.json` uses `oneOf` to enforce kind-specific `utf8`, `bytes`, and `repeat` descriptor requirements.
- Passed: `git diff --check`.
- Passed: `bash .agents/sow/audit.sh`.

Real-use evidence:

- Dataset generation and validation run locally from repository-relative paths.
- The large performance stream was materialized under `.local/datasets/performance-corpus.jsonl` and hashed successfully.
- No SDK writer, SDK reader, journal file writer, or journalctl rewrite was changed in this SOW; writer consumption begins in SOW-0015.

Reviewer findings:

- Round 1 Minimax verdict: `PRODUCTION GRADE`, but it reported `cardinality: "single"` as informational despite schema mismatch evidence. Treated as supporting evidence only because GLM and Mimo independently found blocking schema issues.
- Round 1 GLM verdict: `NOT PRODUCTION GRADE`. Blocking findings:
  - `tests/datasets/schema.schema.json` rejected `ingestion-manifest.json` because the manifest `determinism` object included `boot_id`, `machine_id`, `invocation_id`, and `json` while schema `additionalProperties: false` did not allow them.
  - `tests/datasets/schema.schema.json` rejected correctness records 0 and 1 because `cardinality: "single"` was not in the schema enum.
  - `tests/datasets/validate.py` loaded the schema but did not enforce it.
  - Low code-smell finding: inline `__import__("base64")`.
- Round 1 Mimo verdict: `NOT PRODUCTION GRADE`. It found the same schema determinism mismatch, `single` enum mismatch, and missing schema enforcement.
- Round 1 disposition:
  - Added deterministic ID and JSON-format properties to the schema determinism object.
  - Added `single` to the cardinality enum.
  - Added JSON Schema validation for the top-level manifest, all accepted records, and all rejected records in `tests/datasets/validate.py`.
  - Replaced inline `__import__("base64")` with a normal top-level import.
- Round 2 Minimax, GLM, and Mimo verdicts: `PRODUCTION GRADE`.
- Round 2 disposition:
  - Added generation-time `repeat()` range validation.
  - Clarified the `jsonschema` dependency comment.
  - Documented `raw_payload` handling and growth-pressure tag semantics in `tests/datasets/README.md`.
- Round 3 GLM and Mimo verdicts: `PRODUCTION GRADE`.
- Round 3 disposition:
  - Tightened `schema.schema.json` to require `invocation_id` for accepted records.
  - Tightened `schema.schema.json` value descriptors with `oneOf` for kind-specific validation.
- Round 4 Minimax, GLM, and Mimo verdicts: `PRODUCTION GRADE`.
- Round 4 false-positive dispositions:
  - Minimax reported a seed mismatch, but `python3 -c 'print(0x5D17A5EED)'` returns `24989294317`, matching the committed manifests.
  - Mimo reported that systemd requires field names to be at least two bytes, but `systemd/systemd @ cf3156842209 src/libsystemd/sd-journal/journal-file.c:1722` only rejects empty field names with `l <= 0`; one-byte names are valid.
  - GLM looked for `.agents/sow/SOW-status.md`, but the project status file is the root `SOW-status.md`.

Same-failure scan:

- Dataset validator checks all required correctness and rejection coverage tags before the dataset can be accepted.
- Independent JSON Schema validation checks every committed accepted and rejected record.
- No SDK writer behavior was changed in this SOW.

Sensitive data gate:

- Dataset rows are synthetic.
- Changed-file scan for user names, common token prefixes, credential assignments, private-key markers, and sensitive data patterns produced no matches.
- Reviewer scans found no security or sensitive-data issue.

Artifact maintenance gate:

- AGENTS.md: no update needed; repository workflow did not change.
- Runtime project skills: no update needed; this SOW adds data artifacts, not a reusable work procedure beyond SOW-0015 consumption.
- Specs: no public product behavior changed; dataset format is test infrastructure and documented locally.
- End-user/operator docs: `tests/datasets/README.md` documents dataset format, generation, validation, value descriptors, rejection `raw_payload`, and growth-pressure semantics.
- End-user/operator skills: no output/reference skill is produced by this SOW.
- SOW lifecycle: this SOW is marked `completed` and moved to `done/`.
- `SOW-status.md`: updated to remove SOW-0014 from current work and list it as done.
- Lessons: recorded below.
- Followup mapping: SOW-0015 consumes this dataset; SOW-0016 uses it for byte-identical writer comparison; SOW-0009 uses it for benchmark/profile work.

SOW status and directory consistency:

- Before closeout: `Status: completed` is recorded in this file.
- Closeout action: move from `.agents/sow/current/` to `.agents/sow/done/`.
- Final audit passed after the move: `bash .agents/sow/audit.sh`.

## Outcome

Completed.

- Delivered a deterministic, language-neutral ingestion dataset package under `tests/datasets/`.
- Delivered committed correctness and rejection corpora, plus committed metadata for an on-demand 200000-row performance stream.
- Delivered a validator that enforces schema, semantic coverage, hashes, deterministic regeneration, and performance stream hash stability.
- Delivered local dataset README documentation for generator, validator, value descriptors, rejection payloads, and growth-pressure semantics.

## Lessons Extracted

- JSON Schema must be enforced by the validator, not only stored as documentation.
- Reviewer findings about compatibility details must be verified against systemd source before being accepted; two final-round findings were false positives.
- The performance stream can stay uncommitted if the generator, manifest, and hash validation make the stream reproducible.

## Followup Mapping

- `SOW-0015-20260524-deterministic-ingesters.md`: build systemd C and SDK ingesters that consume this dataset.
- `SOW-0016-20260524-byte-identical-writer-compatibility.md`: use accepted records for byte-identical writer comparison against systemd.
- `SOW-0009-20260523-benchmark-profile-optimize.md`: use the performance stream for profiling and optimization.

## Regression Log

None yet.
