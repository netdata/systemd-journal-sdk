# SOW-0064 - Real World Journal Corpus Evaluation

## Status

Status: open

Sub-state: pending tooling SOW for large-scale real journal corpus evaluation.

## Requirements

### Purpose

Build a streaming real-world evaluation harness that proves, on the
workstation's large local journal corpus, whether the SDK readers and writers
preserve journal information, remain compatible with stock systemd tooling, and
perform operationally equal to or better than systemd at realistic scale.

### User Request

The user stated that this workstation has more than 100 GB of journal files and
requested a simple but rigorous experiment:

- read all files with the Rust reader and regenerate them with the Rust writer;
- repeat comparable read/write regeneration with systemd, Rust, and Go;
- exclude Python and Node.js from this large corpus path because they are too
  slow for this scale;
- report hashes, counts, speed/rate, peak memory, average and peak I/O, total
  disk bandwidth including page-fault driven I/O, and output disk footprint;
- verify reader completeness by matching hashes and counts with the systemd
  baseline;
- verify writer completeness by regenerating files and rereading generated
  files with systemd;
- compare reader speed, writer speed, memory footprint, I/O behavior, disk I/O
  multiplication, compact vs regular footprint, compact compressed footprint,
  compact FSS footprint and speed penalty, and byte-by-byte identity where
  applicable;
- do this incrementally because there is not enough disk space to copy the full
  corpus multiple times;
- make this SOW about building tooling and identifying problems, not fixing
  problems.

### Assistant Understanding

Facts:

- The corpus is large enough that the harness must be streaming and
  incremental. Bulk copying the corpus or keeping all regenerated outputs is not
  acceptable.
- Real workstation journal files may contain sensitive operational data. Durable
  artifacts must not store raw entries, raw fields, hostnames, IPs, usernames,
  messages, or binary payloads.
- Systemd is the baseline oracle for readable information, stock verification,
  and C implementation performance.
- Rust and Go are the only SDK implementations in scope for this large
  real-world corpus evaluation.
- Python and Node.js are explicitly out of scope for this corpus-scale
  performance run.
- The purpose is to find discrepancies and quantify them. Fixes belong in
  follow-up SOWs.

Inferences:

- The harness needs at least two independent canonical digests:
  - an information digest for the logical entry stream, independent of on-disk
    object offsets and writer layout;
  - a byte digest for generated journal file bytes, used only when byte identity
    is an expected property for a controlled regeneration mode.
- "All information" should be specified as the file-backed reader-visible
  information that a journal file can expose: entry count, payload count,
  repeated fields, binary field bytes, timestamps, boot IDs, monotonic data,
  sequence metadata where available, output states, compression/FSS/compact
  feature flags, and verification status.
- For arbitrary existing real journals, byte-for-byte identity with systemd may
  not always be a valid pass/fail condition unless the regeneration mode fixes
  all nondeterministic IDs, timestamps, sequence IDs, compression choices,
  sealing keys, state transitions, and final-state behavior. The harness should
  measure byte identity where it is meaningful and classify every mismatch.
- Real corpus files may include historical variants or corrupt/truncated files;
  the harness must classify these rather than aborting the whole run.

Unknowns:

- Exact location and size distribution of the workstation journal corpus to
  evaluate.
- Whether all files are readable by the current user without privilege changes.
- Whether enough free space exists for the largest single-file regeneration plus
  metrics and temporary outputs.
- Whether systemd C regeneration can preserve all selected metadata through
  public APIs or whether a repository-local helper based on systemd internals is
  needed.

### Acceptance Criteria

- Provide a repository-local harness that discovers a journal corpus from
  configured input roots and streams files incrementally without copying the
  whole corpus.
- The harness never writes raw journal content to durable reports. Reports
  contain only paths if allowed, sanitized file identifiers, feature
  classifications, counts, digests, sizes, timings, memory/I/O metrics, and
  discrepancy summaries.
- The harness supports at least these drivers:
  - systemd reader baseline;
  - Rust reader;
  - Go reader;
  - systemd writer/regenerator baseline where technically feasible;
  - Rust writer/regenerator;
  - Go writer/regenerator.
- The harness computes canonical logical digests and counts for original files
  and regenerated files, including binary-safe and repeated-field-aware hashing.
- The harness validates regenerated outputs with stock `journalctl --verify
  --file` where applicable and rereads regenerated outputs with the systemd
  baseline to compare logical digests/counts.
- The harness measures per-file and aggregate:
  - entries/s and payload bytes/s for readers and writers;
  - total elapsed time;
  - peak resident memory;
  - average and peak read/write I/O;
  - filesystem bytes read and written where available;
  - page-fault counts and page-fault-related disk bandwidth where measurable;
  - input bytes, generated bytes, and footprint ratios;
  - I/O multiplication ratios for read and write paths.
- The harness supports regeneration modes for regular, compact, compact plus
  compression, and compact plus FSS where the source information is sufficient.
- The harness compares disk footprint for regular vs compact, compact
  compressed vs compact uncompressed, and compact FSS vs compact without FSS.
- The harness records discrepancies as structured cases suitable for follow-up
  SOW creation, including enough sanitized evidence to reproduce using the
  original local file while avoiding raw data in durable artifacts.
- The harness can resume after interruption and skip files already evaluated
  with matching input file identity and size/mtime/hash metadata.
- The harness has a dry-run/list mode and a small smoke mode before scanning
  the full corpus.
- The harness documents required privileges, free-space requirements, and safe
  cleanup behavior.

## Analysis

Sources checked:

- User request in this thread.
- Existing benchmark and interoperability harness conventions under
  `tests/benchmarks/` and `tests/interoperability/`.
- Existing project compatibility rules in
  `.agents/skills/project-journal-compatibility/SKILL.md`.
- Existing SOW boundary and sensitive-data rules in `AGENTS.md`.

Current state:

- The project has strong synthetic and interoperability validation, but no
  corpus-scale real-world journal evaluation harness.
- Current benchmark artifacts measure controlled datasets, not the full
  workstation corpus with historical variation, real cardinality, real field
  repetition, real binary values, and real file size distribution.
- Existing validation is excellent for compatibility slices but does not answer
  full-corpus operational questions about memory, I/O multiplication,
  page-fault bandwidth, or footprint change across regeneration modes.

Risks:

- Real journals can contain sensitive data. A careless report could leak
  messages, usernames, hostnames, IPs, service names, or binary payloads.
- The workstation may not have enough free space for naive output retention.
- Byte-for-byte identity can be misleading on arbitrary real journals unless
  nondeterminism is controlled; the tool must distinguish logical information
  parity from byte identity.
- Systemd APIs may not expose enough writer control to regenerate byte-identical
  output for all feature modes.
- Page-fault and disk-I/O attribution is OS- and filesystem-dependent; metrics
  may need multiple sources and clear confidence labels.
- Running full-corpus tests may take a long time and put sustained load on the
  workstation disk.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The SDK has passed controlled compatibility and benchmark tests, but those
  tests do not prove behavior against the workstation's large real journal
  corpus. A streaming harness is needed to compare systemd, Rust, and Go on
  logical completeness, regenerated-output validity, performance, memory, I/O,
  and disk footprint without copying the corpus.

Evidence reviewed:

- User-provided corpus size and evaluation goals.
- Existing local benchmark/reporting pattern.
- Existing project rules forbidding raw sensitive data in durable artifacts.
- Existing compatibility skill requiring stock systemd validation for journal
  changes and forbidding live host journal mutation.

Affected contracts and surfaces:

- New corpus evaluation tooling under repository-local tests or tools.
- Rust and Go reader/writer command helpers, if additional corpus-mode commands
  are needed.
- Systemd baseline helper builds or commands.
- Benchmark/report schemas.
- SOW status and follow-up SOW creation workflow.
- Documentation for running the corpus evaluation safely.

Existing patterns to reuse:

- `tests/benchmarks/report_benchmarks.py` style structured reporting.
- Existing `.local/` output convention.
- Existing systemd helper build conventions under `tests/benchmarks/systemd/`
  and dataset ingesters.
- Existing interoperability runners for stock `journalctl --verify --file`.
- Existing JSON result artifacts with summary plus per-case records.

Risk and blast radius:

- Medium for tooling, high for operational load if run across the full corpus.
- The SOW should not change SDK read/write behavior unless a minimal helper API
  is required for measurement.
- The SOW should not attempt fixes for discovered SDK/systemd discrepancies.
- The SOW must not mutate source journal files or host journal directories.

Sensitive data handling plan:

- Input journals are treated as sensitive.
- Do not write raw entries, raw fields, field values, hostnames, usernames,
  process names, command lines, IP addresses, binary payloads, or service names
  into durable SOWs, specs, docs, logs, or reports.
- Reports may include sanitized stable file identifiers, file sizes, feature
  classes, counts, cryptographic digests, timings, resource metrics, and
  discrepancy codes.
- Full raw discrepancy artifacts may only be written to a user-approved,
  uncommitted scratch directory and must be excluded from git; the default is
  no raw discrepancy capture.
- Generated regenerated journal files are temporary by default and deleted after
  validation unless the user explicitly asks to keep a failing case.

Implementation plan:

1. Corpus contract and metric schema.
   - Define canonical logical digest format, count taxonomy, resource metrics,
     I/O multiplication formulas, and discrepancy classes.
2. Discovery and streaming scheduler.
   - Enumerate configured roots, classify files, estimate free-space needs,
     support dry-run/smoke/full modes, and process one file or bounded batches
     at a time.
3. Reader digest drivers.
   - Implement systemd, Rust, and Go logical digest/count drivers with
     identical canonicalization and binary/repeated-field handling.
4. Regeneration drivers.
   - Implement systemd, Rust, and Go regeneration modes for regular, compact,
     compact compressed, and compact FSS where feasible.
5. Validation loop.
   - For each file and mode: read original, regenerate, verify generated file,
     reread generated file with systemd, compare logical digests/counts, record
     metrics, delete output unless retained for failure triage.
6. Resource measurement.
   - Capture elapsed time, peak RSS, CPU time, per-process I/O counters,
     page-fault counters, and filesystem byte deltas where available.
7. Reporting and resume.
   - Produce aggregate and per-file JSON/Markdown summaries without raw data,
     plus resumable state keyed by sanitized file identity.
8. Review and follow-up mapping.
   - Run a small smoke corpus first, then the full corpus if the user approves
     runtime load. Convert every valid discrepancy into a follow-up SOW or an
     explicit rejected/not-actionable record.

Validation plan:

- Unit tests for canonical logical hashing using synthetic entries with binary
  values, repeated fields, empty values, large values, and ordering edge cases.
- Smoke evaluation on a small repository-local synthetic corpus.
- Dry-run against the real corpus to report counts, sizes, feature
  distribution, estimated scratch space, and expected runtime without reading
  full payloads.
- Full run only after dry-run output is reviewed or explicitly approved.
- Cross-check a sample of generated files with stock `journalctl --verify
  --file`.
- Confirm generated reports contain no raw field values or messages.
- `.agents/sow/audit.sh` and `git diff --check`.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: update only if real-corpus evaluation becomes a
  mandatory future compatibility workflow.
- Specs: update product scope only if the harness establishes new compatibility
  or performance evidence that changes project claims.
- End-user/operator docs: add or update corpus evaluation documentation if the
  tool is intended for operators/developers to run.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: this SOW builds tooling and identifies issues only; follow-up
  SOWs own fixes.
- SOW-status.md: add this SOW to Pending.

Open-source reference evidence:

- No external open-source repositories were checked while creating this SOW.
  Implementation should use systemd v260.1 source/helpers only as a baseline
  and record durable `systemd/systemd @ <commit>` citations if new systemd
  internals are inspected.

Open decisions:

- Resolved by user: this SOW is for tooling/evaluation and discrepancy
  identification, not fixing discovered issues.
- Resolved by user: Python and Node.js are excluded from the large real-corpus
  performance loop.
- To decide before full-corpus execution: exact corpus roots, whether paths may
  be recorded verbatim or must be hashed, allowed runtime/load window, and
  whether raw failing regenerated files may be temporarily retained.

## Implications And Decisions

1. 2026-05-30 real corpus evaluation scope
   - Decision: build tooling to evaluate the workstation's large journal corpus
     with systemd, Rust, and Go only.
   - Implication: the harness must be streaming, resumable, and metrics-only by
     default.
   - Risk: full-corpus execution can be long and I/O intensive, so dry-run and
     smoke modes are mandatory.

2. 2026-05-30 discrepancy handling
   - Decision: this SOW identifies discrepancies; fixes are follow-up SOWs.
   - Implication: the harness must classify and report failures clearly enough
     to open targeted follow-up work.
   - Risk: do not hide known discrepancies by weakening pass/fail criteria.

3. 2026-05-30 sensitive corpus handling
   - Decision: durable artifacts must not contain raw journal content.
   - Implication: reports use hashes, counts, metrics, and sanitized identifiers
     by default.
   - Risk: keeping raw failing files requires explicit user approval and must
     remain uncommitted.

## Plan

1. Define canonical digest and metrics contract.
   - Scope: information-preserving hash, counts, I/O/memory/speed formulas.
   - Risk: bad canonicalization can create false confidence.

2. Build streaming discovery and runner.
   - Scope: roots, file classification, scratch-space checks, resume state,
     one-file-at-a-time regeneration.
   - Risk: accidental bulk copying or output retention.

3. Build systemd/Rust/Go reader digest drivers.
   - Scope: logical read completeness and reader performance.
   - Risk: different enumeration APIs may expose subtly different metadata.

4. Build systemd/Rust/Go writer regeneration drivers.
   - Scope: regular, compact, compressed compact, and compact FSS modes.
   - Risk: byte identity may be meaningful only for controlled submodes.

5. Build validation, reporting, and follow-up extraction.
   - Scope: generated-file verification, reread comparisons, aggregate report,
     discrepancy report, follow-up SOW mapping.
   - Risk: reports must remain useful without leaking raw data.

## Delegation Plan

Implementer:

- Current routing is local implementation. Do not run external implementer
  agents unless the user explicitly changes the routing decision.

Reviewers:

- Use read-only reviewers from the approved pool after implementation and local
  validation complete: `llm-netdata-cloud/minimax-m2.7-coder`,
  `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/qwen3.6-plus`, and
  `llm-netdata-cloud/glm-5.1`. Skip `llm-netdata-cloud/mimo-v2.5-pro` while it
  remains out of quota.

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

- If the real corpus cannot be read due to permissions, record the inaccessible
  counts and paths according to the selected path-sanitization policy.
- If free space is insufficient even for the largest single-file regeneration,
  stop and report required space before running destructive cleanup or changing
  inputs.
- If systemd cannot regenerate a mode with public APIs, record the limitation
  and either implement a repository-local helper based on approved systemd
  internals or split a follow-up decision SOW.
- If metrics such as page-fault disk bandwidth cannot be measured reliably,
  report them with confidence labels and the exact kernel/tool source used.

## Execution Log

### 2026-05-30

- Created this pending SOW from the user's request for a real-world
  corpus-scale verification and performance harness.

## Validation

Acceptance criteria evidence:

- Pending implementation.

Tests or equivalent validation:

- Pending implementation.

Real-use evidence:

- Pending implementation against the workstation's real journal corpus.

Reviewer findings:

- Pending implementation and whole-SOW review.

Same-failure scan:

- Pending implementation.

Sensitive data gate:

- SOW creation records no raw journal content. The SOW explicitly requires
  metrics-only durable reports and treats real journals as sensitive.

Artifact maintenance gate:

- AGENTS.md: no update during SOW creation.
- Runtime project skills: no update during SOW creation.
- Specs: pending implementation; update only if claims or workflow change.
- End-user/operator docs: pending implementation.
- End-user/operator skills: no output/reference skill affected during SOW
  creation.
- SOW lifecycle: created as `Status: open` under `.agents/sow/pending/`.
- SOW-status.md: updated to list this SOW as pending.

Specs update:

- Pending implementation.

Project skills update:

- Pending implementation.

End-user/operator docs update:

- Pending implementation.

End-user/operator skills update:

- Pending implementation.

Lessons:

- Controlled benchmarks are necessary but not sufficient; corpus-scale
  evaluation needs logical digests, resource metrics, and careful sensitive-data
  handling.

Follow-up mapping:

- None yet. Every discrepancy found by the eventual harness must be mapped to a
  fix SOW, rejected with evidence, or marked not reproducible with evidence.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and
later testing or use found broken behavior. Use a dated `## Regression -
YYYY-MM-DD` heading at the end of the file. Never prepend regression content
above the original SOW narrative.
