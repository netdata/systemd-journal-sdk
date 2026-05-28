# SOW-0051 - Node.js And Python Writer Performance

## Status

Status: open

Sub-state: follow-up from SOW-0042. This work is not on the immediate reader
critical path, but it blocks any claim that Node.js and Python writers are
performance-certified for high-throughput ingestion.

## Requirements

### Purpose

Bring Node.js and Python writer performance to a production-grade level for
high-throughput journal ingestion without changing the shared writer file-format
contract or weakening stock systemd compatibility.

### User Request

The user selected option A from SOW-0042: close writer certification with Rust
and Go performance-certified, record Node.js and Python as correctness-certified
but performance-limited, and track Node.js/Python writer optimization as a
follow-up SOW.

### Assistant Understanding

Facts:

- SOW-0042 accepted all-language writer correctness for the compact,
  no-compression, FSS-off production baseline with stock per-file verification
  and stock directory readback.
- SOW-0042 measured Rust and Go above the systemd C writer reference for the
  accepted production-sized direct and directory writer baselines.
- SOW-0042 measured Node.js and Python around 0.9k-1.0k append rows/s on the
  same writer baselines, far below systemd, Rust, and Go.
- The immediate project path moves to reader parity and reader performance
  before Netdata integration.

Inferences:

- Node.js and Python writer performance likely needs profiling around
  allocation churn, byte-buffer construction, hashing, object lookup, file
  access, metadata publication, and per-entry API overhead before any targeted
  optimization is chosen.
- Optimizations must be measured on the same `writer-core` and
  `writer-directory` benchmark surfaces added in SOW-0042.

Unknowns:

- Whether Node.js can approach Rust/Go without native mmap or a native addon.
- Whether Python can approach Rust/Go with the current whole-file mmap arena
  but pure-Python object handling.

### Acceptance Criteria

- Node.js and Python writer hot paths are profiled before optimization.
- Performance changes are made in batches large enough for meaningful review.
- Direct and directory writer benchmarks cover raw full-payload and structured
  append paths.
- Correctness validation remains stock-compatible: generated files pass stock
  `journalctl --verify --file` and stock `journalctl --directory` readback.
- The SOW records measured before/after rows/sec, profiler evidence, and any
  remaining gap against systemd, Rust, and Go.
- If Node.js or Python cannot reach production-grade throughput without native
  runtime code, the SOW records concrete evidence and a user decision.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/current/SOW-0042-20260528-writer-final-certification.md`
- `.agents/sow/specs/product-scope.md`
- `tests/benchmarks/README.md`
- `tests/benchmarks/run_writer_core_benchmarks.py`
- `tests/benchmarks/run_writer_directory_benchmarks.py`
- `node/internal/testcmd/writer-core-bench.js`
- `python/cmd/writer_core_bench.py`

Current state:

- Accepted single-file raw baseline from SOW-0042:
  systemd 35,358 rows/s; Rust 47,248 rows/s; Go 49,937 rows/s; Node.js
  1,020 rows/s; Python 950 rows/s.
- Accepted single-file structured baseline from SOW-0042:
  systemd 37,653 rows/s; Rust 50,066 rows/s; Go 52,051 rows/s; Node.js
  992 rows/s; Python 966 rows/s.
- Accepted directory raw baseline from SOW-0042:
  systemd 31,480 rows/s; Rust 45,242 rows/s; Go 46,105 rows/s; Node.js
  924 rows/s; Python 879 rows/s.
- Accepted directory structured baseline from SOW-0042:
  systemd 32,612 rows/s; Rust 47,324 rows/s; Go 44,816 rows/s; Node.js
  920 rows/s; Python 864 rows/s.

Risks:

- Optimizing without profiling can make the pure-language implementations more
  complex without improving the true hot path.
- Matching Rust/Go throughput may be unrealistic in Node.js or Python without a
  different runtime strategy; this must be proven, not assumed.
- Any shortcut that changes object layout, hash ordering, live publication
  semantics, field-name policy, or directory rotation rules would invalidate
  writer compatibility.

## Pre-Implementation Gate

Status: needs-user-decision

Problem / root-cause model:

- Node.js and Python currently meet writer correctness but not high-throughput
  performance expectations. The root cause is not yet proven; profiling is
  required before implementation.

Evidence reviewed:

- SOW-0042 accepted benchmark reports and reviewer findings.
- Product scope writer API and compatibility contracts.
- Benchmark harnesses introduced and repaired in SOW-0042.

Affected contracts and surfaces:

- Node.js and Python direct-file writer hot paths.
- Node.js and Python high-level directory writer hot paths.
- Raw full-payload and structured append APIs.
- Stock-compatible file format, live publication, rotation, retention, and
  verification behavior.
- Benchmark reports and public performance claims.

Existing patterns to reuse:

- SOW-0042 `writer-core` and `writer-directory` benchmark surfaces.
- SOW-0042 max-size/rotation-size equality guard for comparable directory
  benchmarks.
- Shared stock verification and directory readback validation.

Risk and blast radius:

- Medium to high for SDK maintainability if performance work introduces complex
  special cases.
- High for correctness if object ordering, hash chains, DATA deduplication,
  or live publication are altered without full matrix validation.

Sensitive data handling plan:

- Use generated deterministic benchmark data only. Do not record real logs,
  SNMP communities, customer data, credentials, bearer tokens, personal data,
  private endpoints, or production incident content.

Implementation plan:

1. Re-run SOW-0042 Node.js/Python writer baselines to confirm the starting
   point on the current code.
2. Profile Node.js and Python direct writer hot paths separately from directory
   writer paths.
3. Identify allocation, hashing, lookup, byte-buffer, and file-access hot spots.
4. Implement measured optimizations without changing public compatibility
   contracts.
5. Re-run correctness and performance validation.

Validation plan:

- Node.js and Python package tests.
- Shared writer-core and writer-directory benchmarks.
- Stock `journalctl --verify --file` and stock `journalctl --directory`
  validation.
- Relevant interoperability matrices for any touched writer behavior.
- Read-only reviewer passes.
- `git diff --check`.
- `.agents/sow/audit.sh`.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: update only if a durable benchmark/profiling workflow
  changes.
- Specs: update Node.js/Python writer performance status after the outcome.
- End-user/operator docs: update benchmark docs if command surfaces change.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: close only after profiling, optimization, validation, and
  reviewer disposition.
- SOW-status.md: update when this SOW activates or closes.

Open-source reference evidence:

- No new external open-source references checked while creating this follow-up.
  The follow-up is based on repository-local SOW-0042 evidence.

Open decisions:

- Define the target threshold before implementation starts: equal to systemd,
  within a percentage of systemd, or best effort with evidence if runtime
  limits prevent parity.

## Implications And Decisions

- 2026-05-29: user selected SOW-0042 option A: Rust and Go are writer
  performance-certified; Node.js and Python are writer correctness-certified
  but performance-limited; Node.js/Python writer optimization is tracked here
  instead of blocking reader work.

## Plan

1. Confirm target threshold and activation timing.
2. Profile current Node.js and Python writer baselines.
3. Optimize measured hot paths in meaningful batches.
4. Re-run benchmarks and compatibility validation.
5. Record outcome and update specs.

## Delegation Plan

Implementer:

- Local implementation unless the user changes routing.

Reviewers:

- Read-only reviewers from the approved pool.

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

- If profiling shows a runtime limitation or a compatibility-preserving
  optimization ceiling, record evidence and ask the user for a threshold or
  product-positioning decision before claiming production-grade performance.

## Execution Log

### 2026-05-29

- Created as a follow-up from SOW-0042 option A.

## Validation

Acceptance criteria evidence:

- Pending activation.

Tests or equivalent validation:

- Pending activation.

Real-use evidence:

- Pending activation.

Reviewer findings:

- Pending activation.

Same-failure scan:

- Pending activation.

Sensitive data gate:

- No sensitive data was written while creating this SOW. Baseline figures come
  from synthetic benchmark reports referenced by SOW-0042.

Artifact maintenance gate:

- AGENTS.md: no update needed for SOW creation.
- Runtime project skills: no update needed for SOW creation.
- Specs: product scope updated by SOW-0042 to record the current limitation.
- End-user/operator docs: no update needed for SOW creation.
- End-user/operator skills: no update needed.
- SOW lifecycle: pending SOW created.
- SOW-status.md: updated by SOW-0042 close.

Specs update:

- Pending activation outcome.

Project skills update:

- No project-skill update identified.

End-user/operator docs update:

- No docs update identified.

End-user/operator skills update:

- No end-user/operator skill exists for this benchmark surface.

Lessons:

- Pending activation.

Follow-up mapping:

- Pending activation.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.
