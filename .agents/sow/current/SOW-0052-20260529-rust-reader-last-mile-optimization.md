# SOW-0052 - Rust Reader Last-Mile Optimization

## Status

Status: in-progress

Sub-state: created from the user's 2026-05-29 priority change. Rust reader is
the active optimization target before Python and Node.js reader/writer ports.

## Requirements

### Purpose

Make the Rust reader the fastest and cleanest reference reader this SDK can
provide while preserving libsystemd/Netdata `jf` compatibility. This is the
reference implementation that Python and Node.js reader ports will follow.

### User Request

The user asked to optimize the Rust reader as much as practical first, then
port the Rust reader and writer behavior to Python, and finally to Node.js.

### Assistant Understanding

Facts:

- Rust reader parity with libsystemd/Netdata `jf` is complete in SOW-0043.
- Rust reader hot-path optimization already fixed live-bound refresh overhead
  in SOW-0044.
- The user now wants one more Rust-first optimization pass before Python and
  Node.js are aligned to Rust.
- Python and Node.js reader/writer work must follow the Rust contract, not a
  temporary or less optimized contract.

Inferences:

- This SOW should focus on measurable Rust reader bottlenecks that remain after
  SOW-0043 and SOW-0044.
- The optimization goal is not speculative complexity. Changes should be kept
  only when benchmarks or profiles prove they help without weakening
  compatibility or API clarity.

Unknowns:

- Whether meaningful Rust reader bottlenecks remain after SOW-0044.
- Whether the best remaining gains are in the native SDK path, facade path,
  directory merge path, compression path, or query/filter path.

### Acceptance Criteria

- Current Rust reader benchmarks are re-established for single-file and
  ordered directory reads.
- Profiling identifies the remaining Rust reader hot paths before changes.
- Rust reader optimizations are implemented only when supported by measured
  evidence.
- libsystemd-compatible facade semantics from SOW-0043 remain unchanged:
  uncompressed current-entry data is mmap-backed, compressed current-entry data
  uses one reusable decompression buffer, and pointer lifetime is reader-owned.
- Shared reader conformance, directory, mixed-directory, and live-reader
  compatibility tests still pass for affected Rust surfaces.
- Benchmarks are rerun and recorded after optimization, including comparison to
  stock libsystemd where the harness supports it.
- Documentation/specs are updated if public reader behavior or performance
  claims change.
- Read-only external reviewers review the whole SOW after local validation.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/done/SOW-0043-20260528-rust-reader-libsystemd-jf-parity.md`
- `.agents/sow/done/SOW-0044-20260528-rust-reader-hot-path-optimization.md`
- `.agents/sow/current/SOW-0009-20260523-benchmark-profile-optimize.md`
- `.agents/sow/specs/product-scope.md`
- `rust/src/journal/src/lib.rs`
- `tests/benchmarks/run_reader_core_benchmarks.py`

Current state:

- SOW-0043 records Rust single-file `facade-data` live/windowed at about
  1.17M rows/s versus stock libsystemd data enumeration at about 645k rows/s on
  the compact 100k-row benchmark.
- SOW-0044 records Rust `sdk-payloads` live/windowed at about 1.34M rows/s
  versus stock libsystemd data enumeration at about 660k rows/s, with live
  bounds no longer refreshing on every read slice.
- Rust is already ahead of stock libsystemd on the measured single-file and
  open-file benchmark slices, but this does not prove no remaining hot-path
  gains exist.

Risks:

- Over-optimizing Rust could make the reference implementation harder to port
  cleanly to Python and Node.js.
- Changes to facade lifetime handling can silently break libsystemd-like users.
- Changes to directory ordering or live bounds can break live-reader
  compatibility while improving a synthetic benchmark.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The Rust reader is currently production-fast on the measured benchmark
  slices, but the user wants a final Rust-first pass before dynamic-language
  ports. The root-cause model is not a known bug; this is a profiling-driven
  search for remaining avoidable syscalls, allocations, decompression churn,
  entry/object lookups, facade copies, and directory-merge overhead.

Evidence reviewed:

- `.agents/sow/done/SOW-0043-20260528-rust-reader-libsystemd-jf-parity.md`
  records the libsystemd/`jf` facade contract and latest facade benchmark.
- `.agents/sow/done/SOW-0044-20260528-rust-reader-hot-path-optimization.md`
  records the prior live-bound optimization and validation gates.
- `.agents/sow/current/SOW-0009-20260523-benchmark-profile-optimize.md`
  requires separate single-file and ordered directory reader benchmarks.
- `.agents/sow/specs/product-scope.md` records current reader API and
  compatibility contracts.

Affected contracts and surfaces:

- Rust native reader API.
- Rust libsystemd-compatible facade API.
- Rust ordered directory reader.
- Reader benchmark harnesses and benchmark reports.
- Product scope specs and Rust README if public reader behavior is clarified.
- Follow-on Python and Node.js port SOWs.

Existing patterns to reuse:

- `tests/benchmarks/run_reader_core_benchmarks.py`.
- Existing Rust `ReadMode::Live` and snapshot/windowed/whole-file options.
- Existing Rust facade tests around `entry_data_restart()` and
  `enumerate_entry_payload()`.
- Existing mixed-directory and live interoperability harnesses.

Risk and blast radius:

- Medium. The work touches the reference reader hot path used by later ports.
  Incorrect changes could regress compatibility, pointer lifetime, ordered
  directory reads, or live reading.

Sensitive data handling plan:

- Use generated fixtures and repository benchmark artifacts only.
- Do not record real logs, SNMP communities, credentials, bearer tokens,
  customer data, personal data, private endpoints, or production incident
  details.

Implementation plan:

1. Re-run baseline Rust reader benchmarks for single-file and ordered directory
   slices with the current code.
2. Profile representative Rust reader modes: native payload iteration, facade
   data enumeration, ordered directory read, and query/filter path if available
   in the benchmark harness.
3. Inspect hot paths and apply only evidence-backed optimizations.
4. Add focused regression tests for any lifetime, allocation, or ordering
   behavior changed.
5. Re-run Rust tests, compatibility matrices relevant to reader behavior, and
   benchmarks.
6. Run whole-SOW read-only reviewer passes after local validation.

Validation plan:

- `cargo test --manifest-path Cargo.toml --workspace`
- Reader benchmark reruns with recorded command lines and result directories.
- Rust reader/facade targeted tests for any touched behavior.
- Directory, mixed-directory, or live-reader matrix runs when touched code
  affects those surfaces.
- `git diff --check`
- `.agents/sow/audit.sh`
- Whole-SOW read-only reviewer passes.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: no update expected unless a durable profiling rule is
  discovered.
- Specs: update `.agents/sow/specs/product-scope.md` if reader contracts or
  published performance status change.
- End-user/operator docs: update `rust/README.md` only if public reader
  behavior changes or needs clarification.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: this SOW is current/in-progress and should close before Python
  work starts.
- SOW-status.md: update for activation and later close.

Open-source reference evidence:

- No new external open-source repository evidence was needed to create this
  profiling SOW. Existing systemd/libsystemd evidence is inherited from
  SOW-0043 and SOW-0044. If this SOW changes compatibility-sensitive behavior,
  systemd v260.1 source evidence will be cited with durable upstream identity.

Open decisions:

- None. The user already selected the priority order.

## Implications And Decisions

1. 2026-05-29 Rust-first optimization order
   - Decision: optimize Rust reader before Python and Node.js reader/writer
     ports.
   - Implication: Python and Node.js should inherit the final Rust reference
     behavior, reducing duplicated rework.
   - Risk: Rust optimization may find little gain; if so, the correct outcome
     is measured evidence and a clean handoff to Python, not speculative
     complexity.

## Plan

1. Establish baseline and profiles for current Rust reader modes.
2. Implement measured Rust reader optimizations in cohesive batches.
3. Validate tests, compatibility, and benchmarks.
4. Review the complete SOW with read-only reviewers.
5. Commit and push the verified chunk.

## Delegation Plan

Implementer:

- Local implementation by the project manager, per current user routing.

Reviewers:

- Read-only reviewers from the approved pool after the complete SOW is locally
  implemented and validated.

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

- If profiling shows no meaningful Rust bottleneck, record the evidence and
  close with no code changes except benchmark/SOW/spec documentation updates.
- If a reviewer finds a production-grade blocker, fix and rerun the same whole
  SOW review scope.

## Execution Log

### 2026-05-29

- Created from the user's priority change.

## Validation

Acceptance criteria evidence:

- Pending implementation.

Tests or equivalent validation:

- Pending implementation.

Real-use evidence:

- Pending implementation.

Reviewer findings:

- Pending implementation.

Same-failure scan:

- Pending implementation.

Sensitive data gate:

- Planning text contains no raw sensitive data.

Artifact maintenance gate:

- Pending implementation.

Specs update:

- Pending implementation.

Project skills update:

- Pending implementation.

End-user/operator docs update:

- Pending implementation.

End-user/operator skills update:

- Pending implementation.

Lessons:

- Pending implementation.

Follow-up mapping:

- Pending implementation.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.

