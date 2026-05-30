# SOW-0064 - Real World Journal Corpus Evaluation

## Status

Status: completed

Sub-state: implementation and review commits are already merged into `master`;
this SOW is closed.

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

Official documentation evidence checked during implementation:

- `https://systemd.io/JOURNAL_EXPORT_FORMATS/`
  - Confirms journal export is binary-safe, that entries may contain repeated
    fields, and that `journalctl -o export` generates the export stream used by
    the systemd baseline driver.
- `https://www.freedesktop.org/software/systemd/man/latest/systemd-journal-remote.service.html`
  - Confirms `systemd-journal-remote` consumes Journal Export Format input and
    can write a journal file. The harness records systemd regeneration as a
    recognized limitation unless that optional helper is explicitly enabled in
    a later phase.

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
  `llm-netdata-cloud/glm-5.1`. User explicitly re-added
  `llm-netdata-cloud/mimo-v2.5-pro` for the whole-SOW review cycle on
  2026-05-30.

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
- Confirmed user-authorized parallel implementation routing; AGENTS.md
  external-implementer exception applies for this worktree.
- Moved this SOW from `.agents/sow/pending/` to `.agents/sow/current/` and set
  `Status: in-progress`; the closure update later moved it to `done/` with
  `Status: completed`.
- Implemented the metrics-only corpus harness under `tests/corpus_eval/`.
- Implemented a shared canonical logical digest schema:
  `systemd-journal-sdk-corpus-logical-v1`.
- Implemented binary-safe systemd export parsing that treats `_BOOT_ID` as
  common entry metadata instead of a raw payload, matching the Rust/Go reader
  API surface.
- Implemented Rust helper commands:
  `rust/src/internal/testcmd/corpus_digest/` and
  `rust/src/internal/testcmd/corpus_regenerate/`.
- Implemented Go helper commands:
  `go/internal/testcmd/corpus_digest/` and
  `go/internal/testcmd/corpus_regenerate/`.
- Added guarded dry-run, smoke, and full-run modes. Full corpus execution
  requires explicit `--allow-full-run`; no full corpus was run.
- Added resume state, sanitized JSON/Markdown reporting, process resource
  metrics, filesystem I/O counters from GNU `time`, page-fault counts, and
  lower-bound page-fault byte estimates.
- Added Rust/Go regeneration for regular, compact, compact zstd, and compact
  FSS modes. Generated FSS files are verified with an in-memory deterministic
  verification key; the key and FSS start timestamp are not written to reports.
- Left `SOW-status.md` untouched per the assigned prompt; status reconciliation
  is left to the orchestrator to reduce merge conflicts.
- Ran the user-requested whole-SOW read-only external review cycle against
  implementation commit `8476e58`.
- Round 1 produced four `PRODUCTION GRADE` votes and one
  `NOT PRODUCTION GRADE` vote. Real findings were fixed locally before rerunning
  validation and reviewers.
- Hardened `journalctl` process handling, JSON helper output parsing,
  regenerator failure containment and cleanup, resume identity matching, Go
  payload sorting, and status output.
- Reran the whole-SOW read-only review cycle after fixes. Four reviewers
  completed normally with `PRODUCTION GRADE`; the first
  `llm-netdata-cloud/minimax-m2.7-coder` round-2 attempt timed out after 30
  minutes without a vote, then a same-scope rerun completed with
  `PRODUCTION GRADE`.

## Validation

Acceptance criteria evidence:

- Streaming discovery and scheduler:
  - `tests/corpus_eval/run_corpus_eval.py` discovers configured roots only and
    processes one input file at a time.
  - `--mode dry-run` writes stat/list-only reports and does not open journal
    payloads.
  - `--mode run` refuses to execute unless `--allow-full-run` is provided.
- Sensitive-data-safe reporting:
  - Reports use sanitized `file_id` values and omit raw input paths by default.
  - Reports contain counts, digests, sizes, metrics, status codes, and
    discrepancy summaries only.
  - Validation confirmed final dry-run and smoke reports do not contain raw
    fixture paths, `MESSAGE=` payloads, deterministic FSS key bytes, or
    `fss_start_usec`.
- Reader drivers:
  - systemd baseline uses `journalctl --file <path> --output=export --all
    --no-pager` and streams stdout into the canonical digest parser.
  - Rust uses `rust/src/internal/testcmd/corpus_digest/`.
  - Go uses `go/internal/testcmd/corpus_digest/`.
- Writer/regeneration drivers:
  - Rust uses `rust/src/internal/testcmd/corpus_regenerate/`.
  - Go uses `go/internal/testcmd/corpus_regenerate/`.
  - systemd regeneration is recognized but marked unsupported by default in
    this harness because safe use needs optional `systemd-journal-remote`
    enablement and separate pipeline metrics handling.
- Canonical digest/count schema:
  - Binary-safe payload hashing uses length-prefixed SHA-256 input.
  - Per-entry payloads are sorted before hashing so undefined field iteration
    order does not create false mismatches.
  - Repeated field names and binary payloads are counted without writing names
    or values to reports.
- Metrics:
  - Per-driver process wall/user/system seconds, max RSS, minor/major page
    faults, filesystem input/output counters, average I/O rates, input bytes,
    generated bytes, footprint ratios, and I/O multiplication ratios are
    recorded where available.
  - Peak I/O rate is explicitly labelled `not-sampled` in this slice rather
    than fabricated.
- Resume/skip:
  - `state.json` records completed file/driver/mode combinations keyed by
    sanitized file identity, size, mtime, and suffix.

Tests or equivalent validation:

- `python -m unittest tests.corpus_eval.test_canonical`
  - Result: passed, 6 tests.
- `python tests/corpus_eval/run_corpus_eval.py --mode dry-run --root
  .local/corpus-eval/smoke-validation-review-fixes/smoke-fixtures --out
  .local/corpus-eval/dry-run-validation-review-fixes --max-files 5`
  - Result: passed.
  - Evidence: 1 `.journal` file discovered, 8,388,608 input bytes, 0 payload
    reads, 0 discrepancies.
- `python tests/corpus_eval/run_corpus_eval.py --mode smoke --out
  .local/corpus-eval/smoke-validation-review-fixes --drivers systemd rust go
  --regenerators rust go --timeout 1800`
  - Result: passed.
  - Evidence: 1 generated synthetic fixture, 11 result rows, 0 discrepancies.
  - Covered reader drivers: systemd, Rust, Go.
  - Covered writer modes: regular, compact, compact zstd, compact FSS for Rust
    and Go.
- `cargo test --manifest-path rust/src/internal/testcmd/corpus_digest/Cargo.toml`
  - Result: passed, helper built and ran 0 unit tests.
- `cargo test --manifest-path
  rust/src/internal/testcmd/corpus_regenerate/Cargo.toml`
  - Result: passed, helper built and ran 0 unit tests.
- `go test ./internal/testcmd/corpus_digest
  ./internal/testcmd/corpus_regenerate`
  - Result: passed, both helper packages compiled.
- `git diff --check`
  - Result: passed.
- `.agents/sow/audit.sh`
  - Result: passed; verdict reported SOW initialization complete and clean.
- Stock systemd used during smoke:
  - `journalctl --version`: `systemd 260 (260.1-2-manjaro)`.

Real-use evidence:

- No full workstation corpus run was performed, per the assigned prompt.
- Smoke mode exercised actual stock `journalctl --file --output=export` and
  `journalctl --verify --file` on a generated repository-local journal under
  `.local/`.
- Dry-run mode was exercised against the smoke fixture directory and persisted
  only sanitized file IDs, sizes, suffix counts, and scratch-space estimates.

Reviewer findings:

- Round 1 against commit `8476e58`:
  - `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`.
    - Disposition: no blocking findings. Cleanup/free-space and process-risk
      observations were covered by the hardening changes below.
  - `llm-netdata-cloud/qwen3.6-plus`: `NOT PRODUCTION GRADE`.
    - Finding: Rust `payloads_without_separator` could double-count empty-name
      payloads.
      - Disposition: rejected as a false positive for the original code because
        the `else` arm for missing `=` and the `eq == 0` branch were mutually
        exclusive. To make intent unambiguous, Rust counting was refactored to a
        single filtered branch in
        `rust/src/internal/testcmd/corpus_digest/src/main.rs`, and a synthetic
        test was added in `tests/corpus_eval/test_canonical.py`.
    - Finding: `json_from_stdout` could silently pick the wrong JSON-like line.
      - Disposition: fixed in `tests/corpus_eval/run_corpus_eval.py`; helper
        stdout must now contain exactly one JSON object line.
    - Finding: Go helper used an O(n^2) payload sort.
      - Disposition: fixed in `go/internal/testcmd/corpus_digest/main.go` by
        using `sort.Slice` with `bytes.Compare`.
    - Finding: resume identity could be stale for recreated files with matching
      size and mtime.
      - Disposition: fixed in `tests/corpus_eval/run_corpus_eval.py` by adding
        `ctime_ns` to both sanitized file ID input and resume identity.
    - Finding: main status output reported the mode string as a report path.
      - Disposition: fixed in `tests/corpus_eval/run_corpus_eval.py`; stdout now
        reports `report_json` and `report_md`.
  - `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`.
    - Disposition: no blocking findings.
  - `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE`.
    - Disposition: no blocking findings. Resume-identity and systemd-version
      observations were covered or remain documented non-blocking risks.
  - `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`.
    - Finding: `systemd_digest` timeout paths could leave `journalctl` running
      and stderr could fill its pipe.
      - Disposition: fixed in `tests/corpus_eval/run_corpus_eval.py`; stdout
        parsing and stderr digesting now run concurrently, timeout paths kill and
        wait for the exact child process, and reports retain only stderr hashes.
    - Finding: unexpected errors during generated-file verification/reread could
      abort the entire evaluation.
      - Disposition: fixed in `tests/corpus_eval/run_corpus_eval.py`; writer
        regeneration, stock verify, systemd reread, generated-size stat, and
        cleanup are now contained as per-case `failed` results.
- Round 2 after fix commit `e49a7d9`:
  - `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`.
    - Finding: Rust regeneration cannot preserve per-entry boot IDs for
      multi-boot journals with the current writer helper API.
      - Disposition: non-blocking for this SOW because the harness will surface
        that as a real corpus writer discrepancy; fixing Rust writer metadata
        preservation belongs in a follow-up SOW if the full corpus finds it.
    - Finding: regeneration helpers do not preserve arbitrary original sequence
      numbers.
      - Disposition: non-blocking known interpretation risk. The harness hashes
        `__SEQNUM` deliberately, so sequence-number renumbering is visible rather
        than hidden.
  - `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`.
    - Finding: writer digest comparison may produce expected discrepancies on
      real files with sequence-number gaps.
      - Disposition: non-blocking; record as a report-interpretation risk for
        full-corpus use and map to follow-up if it prevents useful triage.
    - Finding: archived-mode failure could leave renamed generated files.
      - Disposition: not applicable to the implemented harness modes because the
        CLI supports only `regular`, `compact`, `compact-zstd`, and
        `compact-fss`, and `regenerate_cmd` always requests `--final-state
        offline`.
    - Finding: `display_path` can print an absolute path when `--out` is outside
      the repository.
      - Disposition: non-blocking CLI-only leak risk. Durable reports remain
        path-sanitized; normal documented output path is under `.local/`.
    - Finding: fixed 5-second thread join could be tight for pathological very
      large single entries.
      - Disposition: non-blocking; it fails closed with `TimeoutError` and does
        not leak a child process.
  - `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`.
    - Finding: silent exclusion of non-selected `__`-prefixed systemd export
      fields could surprise future maintainers.
      - Disposition: non-blocking. This is intentional for the current SDK
        reader surface; future systemd metadata drift can be handled with a
        focused schema update.
  - `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`.
    - Disposition: no blocking findings. The reviewer verified the process,
      cleanup, JSON parsing, identity, Go sort, Rust count, status-output, and
      validation fixes.
  - `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE`.
    - Note: the first round-2 attempt timed out before producing a vote; a
      same-scope rerun completed successfully.
    - Finding: the insufficient-scratch-space error string includes byte counts
      before being hashed into `error_sha256`.
      - Disposition: non-blocking. The raw string is not written to durable
        reports; only the hash is stored. Treat as a residual crash-dump/in-memory
        disclosure risk.
- Final reviewer state: all five approved reviewers voted `PRODUCTION GRADE`.

Same-failure scan:

- Smoke validation initially found a digest schema mismatch: systemd export
  exposed `_BOOT_ID` while Rust/Go SDK readers expose boot ID as entry
  metadata. The schema was corrected to canonicalize `_BOOT_ID` as
  `__BOOT_ID` metadata. The final smoke run passed with 0 discrepancies.
- All-mode smoke validation initially found compact FSS stock verification
  failures because sealed files require `journalctl --verify-key`. The harness
  was corrected to use an in-memory deterministic verification key for
  generated FSS files and to avoid writing that key to reports. The final smoke
  run passed with 0 discrepancies.
- Search confirmed final smoke/dry-run reports did not contain raw fixture
  paths, `MESSAGE=` payloads, deterministic FSS key bytes, or `fss_start_usec`.
- Review-fix validation confirmed
  `.local/corpus-eval/smoke-validation-review-fixes/report.*` and
  `.local/corpus-eval/dry-run-validation-review-fixes/report.*` contain none of
  those raw/sensitive markers.
- Review-finding pattern scan checked the changed scope for the same classes:
  `subprocess.Popen`, `json_from_stdout`, `payloads_without_separator`,
  `sortPayloads`, `safe_file_id`, `state.json`, and `run_regenerator`. No second
  unhandled instance of these patterns was found outside the fixed sites.

Sensitive data gate:

- SOW creation records no raw journal content. The SOW explicitly requires
  metrics-only durable reports and treats real journals as sensitive.

Artifact maintenance gate:

- AGENTS.md: no update needed; existing repository-boundary, SOW lifecycle,
  sensitive-data, and external-agent rules covered this work.
- Runtime project skills: no update needed; no durable workflow rule changed.
- Specs: no product-scope update needed; this adds evaluation tooling and does
  not change SDK public contracts or compatibility claims.
- End-user/operator docs: added `tests/corpus_eval/README.md` for safe dry-run,
  smoke, and guarded full-run usage.
- End-user/operator skills: no output/reference skill affected.
- SOW lifecycle: moved from `pending/open` to `current/in-progress`, then closed
  as `completed` and moved to `done/`.
- SOW-status.md: reconciled by the closure update.

Specs update:

- No spec update was needed because no product behavior, SDK API, file format,
  or compatibility guarantee changed. The corpus harness measures existing
  behavior and records discrepancies for follow-up SOWs.

Project skills update:

- No project skill update was needed. The existing journal compatibility and
  orchestration skills already require safe stock-reader validation,
  repository-boundary handling, and sensitive-data handling.

End-user/operator docs update:

- Added `tests/corpus_eval/README.md`.

End-user/operator skills update:

- No end-user/operator skill exists for this tooling and none was required by
  this SOW.

Lessons:

- Controlled benchmarks are necessary but not sufficient; corpus-scale
  evaluation needs logical digests, resource metrics, and careful sensitive-data
  handling.

Follow-up mapping:

- systemd writer/regenerator baseline is recognized but remains unsupported by
  default until a later SOW explicitly enables `systemd-journal-remote` pipeline
  execution and pipeline-level metrics. This is a harness limitation, not an SDK
  discrepancy.
- Peak per-process I/O rate is not sampled in this slice; reports explicitly
  record `peak_io_source: not-sampled` instead of fabricating a number. If
  peak I/O rate becomes a hard acceptance metric for full-corpus runs, create a
  follow-up SOW for `/proc/<pid>/io` sampling across direct commands and
  pipelines.
- Real corpus writer discrepancies involving `__SEQNUM` gaps or multi-boot
  per-entry boot IDs should be interpreted as metadata-preservation findings, not
  raw payload loss. If those dominate the full-corpus report, create a follow-up
  SOW for a secondary comparison mode or writer metadata preservation work.
- CLI status output for `--out` outside the repository may print an absolute
  report path. This is not a durable-report leak; change it later only if
  external output directories become a documented workflow.
- No SDK reader/writer discrepancies were found in final smoke validation.

## Outcome

Implemented locally, fixed real whole-SOW review findings, reran the whole-SOW
read-only reviewer pool, merged the implementation into `master`, and closed
the SOW as `completed`.

## Lessons Extracted

- Treating systemd export fields as raw DATA payloads can create false
  mismatches. `_BOOT_ID` must be normalized as entry metadata for parity with
  the Rust/Go reader APIs.
- FSS verification needs `journalctl --verify-key`; plain stock verify is not a
  sufficient sealed-output check.
- Sensitive reporting needs filtering at both the command-output and final
  report layers, because helper commands may need transient data that should
  not survive into durable artifacts.

## Followup

- No required implementation follow-up remains for this tooling SOW.
- Systemd regeneration through `systemd-journal-remote` and peak I/O sampling
  are not required to close this harness work. If a future full-corpus run proves
  either is required for acceptance, open a targeted follow-up SOW with evidence
  from that run.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and
later testing or use found broken behavior. Use a dated `## Regression -
YYYY-MM-DD` heading at the end of the file. Never prepend regression content
above the original SOW narrative.
