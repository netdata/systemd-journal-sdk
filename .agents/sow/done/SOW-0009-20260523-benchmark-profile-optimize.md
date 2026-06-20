# SOW-0009 - Benchmark Profile Optimize

## Status

Status: completed

Sub-state: completed on 2026-06-21 as an umbrella closeout. Writer and reader
performance work was split into focused child SOWs, and the linked child SOWs
are now completed or closed. This closeout only fixes lifecycle and mapping
records; it changes no SDK behavior.

## Requirements

### Purpose

Benchmark, profile, and optimize SDK writer and reader hot paths only after the
relevant API and compatibility surfaces are stable. Performance work must be
fit for Netdata production replacement, not a synthetic score detached from the
real writer and reader contracts.

### User Request

The user requested writer and reader performance validation against systemd C
and Netdata's current vendored Rust implementation. The user later clarified:

- finish feature and compatibility work before optimization;
- split writers and readers;
- writer baseline first: compact format, compression disabled, FSS disabled;
- reader benchmarks must separately stress single-file readers and ordered
  directory readers;
- systemd C is the reference floor, and Rust/Go should be substantially faster
  for hot Netdata use cases;
- do not use invalid benchmark settings such as accidental 2 GiB single-file
  defaults when production uses explicit max-size behavior;
- do not investigate why v0.1.0 was slow as a current priority.

### Assistant Understanding

Facts:

- On 2026-05-28, after `v0.3.0` / `go/v0.3.0`, the user reported the Netdata
  SNMP traps integration benchmark improved from about 5.5k traps/s on
  `v0.1.0` to about 170k traps/s on `v0.3.0`.
- That result is strong real integration evidence, but it is not a controlled
  SOW-0009 benchmark report.
- Writer work must not continue using obsolete v0.1.0 root-cause investigation
  as an acceptance criterion.
- At the 2026-05-28 rescope, reader performance work had not started yet. It
  later closed through the child SOWs recorded in this closeout.

Inferences:

- SOW-0009 should not be one giant implementation SOW. It should hold the
  performance program and point to focused child SOWs.
- Writer optimization and reader optimization should have independent baseline
  commands, reports, profiles, and closure gates.
- Netdata integration should wait until the relevant writer and reader
  performance child SOWs pass, or until the user explicitly accepts a staged
  exception.

Unknowns:

- Final production pass/fail thresholds per Netdata component.
- Final benchmark hardware normalization and CPU governor policy.
- Final Rust/Go pass/fail thresholds for each Netdata component.

### Acceptance Criteria

- This umbrella SOW remains the performance program index until child SOWs
  close.
- Writer benchmarking and optimization are tracked by SOW-0042 and SOW-0062.
- Rust reader baseline/parity and optimization are tracked by SOW-0043,
  SOW-0044, SOW-0052, and SOW-0060.
- Go reader alignment and optimization are tracked by SOW-0045 and SOW-0056.
- Historical Python/Node reader/writer port work is superseded by SOW-0116 for
  product planning.
- Netdata component integration remains in separate SOWs and requires explicit
  authorization for any Netdata repository changes.
- Benchmark reports must record production-relevant settings: file size,
  directory rotation, compact/regular mode, compression, FSS, live publication
  cadence, sync/flush cadence, field count, value cardinality, binary payloads,
  source realtime handling, and retention policy.
- Benchmark reports must separate SDK time from caller/worker overhead.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/current/SOW-0037-20260527-reference-drift-audit.md`
- `.agents/sow/pending/SOW-0026-20260526-netdata-sdk-integration.md`
- `.agents/sow/pending/SOW-0039-20260528-raw-byte-field-name-reader-representation.md`
- `.agents/sow/specs/product-scope.md`

Current state:

- Writer implementation was substantially improved by `v0.3.0`, based on the
  user-reported SNMP traps result.
- Controlled writer certification and later absolute writer performance work
  closed in SOW-0042 and SOW-0062.
- Reader parity, reader optimization, and the row-scoped current-entry facade
  lifetime follow-up closed in SOW-0043, SOW-0044, SOW-0045, SOW-0052,
  SOW-0056, SOW-0060, and SOW-0061.

Risks:

- Running broad benchmarks before compatibility closure can produce numbers
  that become invalid after later API or format changes.
- Combining writer and reader optimization in one SOW makes it hard to isolate
  regressions.
- Netdata integration before reader performance could regress
  `systemd-journal.plugin`, `otel-signal-viewer.plugin`, or NetFlow query paths.

## Pre-Implementation Gate

Status: satisfied; child SOWs owned implementation gates

Problem / root-cause model:

- Performance work was previously too broad and mixed writer and reader paths.
  The user correctly identified that feature work should finish first, and that
  writer/read baselines must be separate to be meaningful.

Evidence reviewed:

- User-provided SNMP traps benchmark result after `v0.3.0`.
- Existing SOW and product-scope spec inventory.
- Prior benchmark notes retained in this SOW history are treated as context, not
  current pass/fail evidence, unless a child SOW reproduces them with valid
  production settings.

Affected contracts and surfaces:

- Writer and reader benchmark harnesses.
- Rust and Go SDK performance claims.
- Netdata replacement readiness for NetFlow, OTEL, SNMP traps, signal viewer,
  and no-libsystemd systemd journal reading.

Existing patterns to reuse:

- SOW-0014 deterministic dataset.
- SOW-0015 ingesters.
- Existing conformance/interoperability suites.
- Existing `.local/benchmarks/` result convention.

Risk and blast radius:

- High. Performance acceptance controls whether Netdata replaces existing
  vendored journal code.

Sensitive data handling plan:

- Use generated fixtures and sanitized Netdata-shaped data only.
- Do not record real traps, flow payloads, customer data, personal data,
  credentials, bearer tokens, SNMP communities, private endpoints, or production
  logs.

Implementation plan:

1. Keep this SOW paused as the performance index until child SOWs close.
2. Complete writer closure and all-language writer parity SOWs.
3. Complete SOW-0042 for writer benchmark/certification.
4. Complete reader parity and reader performance SOWs.
5. Update this umbrella SOW when child SOWs change the performance program.
6. Close this umbrella once all linked child SOWs are completed or closed.

Validation plan:

- SOW audit after restructuring.
- Child SOW validation records actual benchmark commands, profiles, and results.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: no update expected unless benchmark workflow becomes
  a durable operator rule.
- Specs: update only when child SOWs change public performance or API contract.
- End-user/operator docs: update when public benchmark or integration guidance
  is published.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: this stayed current/paused until the performance program was
  closed, then moved to `.agents/sow/done/`.
- SOW-status.md: updated by the restructuring and closeout records.

Open-source reference evidence:

- No external open-source source was newly checked for this rescope. Existing
  systemd evidence is tracked in the relevant child SOWs.

Open decisions:

- None blocking this rescope. Production thresholds remain child-SOW decisions.

## Implications And Decisions

1. 2026-05-28 split writer and reader performance
   - Decision: SOW-0009 becomes an umbrella. Child SOWs own implementation and
     benchmark closure.
   - Implication: writer work can close before reader work starts, but Netdata
     integration still waits for both relevant paths.

2. 2026-05-28 v0.1.0 root-cause investigation
   - Decision: remove v0.1.0 slowness explanation from current acceptance.
   - Implication: the user-reported v0.3.0 SNMP traps improvement is recorded
     as useful context, not a required reverse-engineering task.

## Plan

1. Complete SOW-0037 writer reference closure.
2. Complete SOW-0040 and SOW-0041 writer parity gaps.
3. Complete SOW-0042 writer final certification and benchmarks.
4. Complete SOW-0043 reader parity.
5. Complete SOW-0052 Rust reader last-mile optimization.
6. Complete SOW-0060 Rust reader absolute hot-path profiling. Completed on
   2026-05-29.
7. Treat historical Python/Node port work as superseded by SOW-0116 for
   product planning.
8. Use the results to unblock SOW-0026 and component Netdata integration SOWs.

## Delegation Plan

Implementer:

- Local implementation by the project manager unless the user changes routing.

Reviewers:

- Read-only reviewers from the approved pool for child SOWs.

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

- Child SOW failures are recorded in their own SOWs and summarized here only
  when they change program sequencing.

## Execution Log

### 2026-05-28

- Rescoped this file as the umbrella performance program after user agreement.

### 2026-05-29

- Added SOW-0060 as the active Rust reader absolute hot-path profiling pass
  after the user clarified that optimized reader paths may use
  snapshot-at-query-start semantics and whole-file mmap when those choices
  improve performance without sacrificing accuracy or robustness.
- SOW-0060 completed the Rust absolute reader hot-path profiling pass,
  implemented Rust ordered-directory non-overlap sequential stepping, and
  established the Rust row-scoped current-entry facade payload lifetime. Go
  parity for that strengthened lifetime remains tracked by this
  reader-performance umbrella before the reader phase can close.
- SOW-0061 completed Go facade DATA enumeration parity for the Rust row-scoped
  current-entry payload lifetime contract. Public docs, product specs, and
  tests now record that cached current-row payload references/objects remain
  valid after end-of-row enumeration and until the
  next row/lifecycle boundary.
- Added SOW-0062 as the active Rust-first, Go-second writer absolute
  performance pass after the user stated that Netdata NetFlow, OTEL logs, and
  SNMP traps ingestion require the fastest possible compatible writers, not
  merely writers that beat systemd.

### 2026-06-21

- Closed this umbrella after verifying the linked child SOWs are completed or
  closed:
  - SOW-0042 - Writer Final Certification.
  - SOW-0043 - Rust Reader Libsystemd/Jf Parity.
  - SOW-0044 - Rust Reader Hot-Path Optimization.
  - SOW-0045 - Go Reader Alignment Optimization.
  - SOW-0052 - Rust Reader Last-Mile Optimization.
  - SOW-0056 - Go Reader Hot-Path Optimization Phase 2.
  - SOW-0060 - Rust Reader Absolute Hot-Path Profiling.
  - SOW-0061 - Cross-Language Row-Scoped Facade Lifetime.
  - SOW-0062 - Rust And Go Writer Absolute Performance.
- Recorded SOW-0056 as related completed Go reader performance work because it
  referenced this umbrella as performance context, even though the older
  SOW-0009 follow-up list did not name it.
- No implementation, benchmark, source, spec, or public documentation changed
  during this closeout.

## Validation

Acceptance criteria evidence:

- The umbrella requirement that child SOWs own implementation and benchmark
  closure is satisfied:
  - SOW-0042 completed writer final certification.
  - SOW-0062 completed Rust and Go writer absolute performance.
  - SOW-0043, SOW-0044, SOW-0052, and SOW-0060 completed the Rust reader parity
    and performance chain named by this SOW.
  - SOW-0045 and SOW-0056 completed Go reader alignment and follow-on hot-path
    performance work.
  - SOW-0061 completed the row-scoped current-entry facade payload lifetime
    parity follow-up discovered by SOW-0060.
  - SOW-0116 retired Python and Node.js from product planning, so no remaining
    Python/Node performance work blocks this Rust/Go umbrella.
- Netdata component integration remains in SOW-0048, SOW-0049, and SOW-0050.
  This SDK SOW does not authorize changes outside this repository.

Tests or equivalent validation:

- This is a lifecycle-only closeout; no SDK code changed and no SDK test suite
  was required.
- Child SOWs record their own benchmark, test, reviewer, and compatibility
  validation evidence.
- Closeout validation command: `.agents/sow/audit.sh` passed on 2026-06-21
  after the SOW move. Audit reported 7 pending SOWs, no current SOWs, 111 done
  SOWs, clean status/directory consistency, clean sensitive-data guardrail, and
  final verdict `SOW initialization complete and clean`.

Real-use evidence:

- User reported SNMP traps improved from about 5.5k traps/s on `v0.1.0` to
  about 170k traps/s on `v0.3.0`; this remains contextual real-use evidence.

Reviewer findings:

- No external reviewer rerun was required for this lifecycle-only closeout.
  Implementation and performance child SOWs carried their own reviewer gates.

Same-failure scan:

- Checked direct and reverse SOW-0009 references across `.agents/sow/current`,
  `.agents/sow/pending`, `.agents/sow/done`, and `.agents/sow/SOW-status.md`.
- Corrected stale closeout mapping by adding SOW-0060 and SOW-0061, and by
  recording SOW-0056 as related completed Go reader performance work.

Sensitive data gate:

- This rescope records no raw secrets, credentials, bearer tokens, SNMP
  communities, customer names, personal data, non-private customer-identifying
  IPs, private endpoints, or proprietary incident details.

Artifact maintenance gate:

- AGENTS.md: no update needed; workflow and repository guardrails did not
  change.
- Runtime project skills: no update needed; no durable HOW-to workflow changed.
- Specs: no update needed; no SDK behavior, API, compatibility, or performance
  contract changed during this closeout.
- End-user/operator docs: no update needed; no public behavior or install/use
  guidance changed during this closeout.
- End-user/operator skills: no output/reference skill affected.
- SOW lifecycle: SOW-0009 is marked `completed` and moved from
  `.agents/sow/current/` to `.agents/sow/done/`.
- SOW-status.md: updated to remove SOW-0009 from Current and add it to Recently
  Closed Or Completed.
- SOW status/directory consistency: verified by `.agents/sow/audit.sh` after the
  move.

Specs update:

- No spec update needed for this closeout; child SOWs already carried any
  behavior or contract updates they required.

Project skills update:

- No project skill update needed; this closeout exposed no missed workflow rule.

End-user/operator docs update:

- No docs update needed; this closeout changes SOW lifecycle state only.

End-user/operator skills update:

- No output/reference skill update needed.

Lessons:

- Performance SOWs must be scoped by hot path and contract; a single broad
  benchmark SOW became too easy to misread.
- Umbrella SOWs should be closed promptly when their child SOWs close, otherwise
  `current/` overstates active work.

Follow-up mapping:

- Writer certification and absolute performance: SOW-0042 and SOW-0062.
- Rust reader parity/performance: SOW-0043, SOW-0044, SOW-0052, and SOW-0060.
- Go reader performance: SOW-0045 and SOW-0056.
- Row-scoped current-entry facade payload lifetime parity: completed by
  SOW-0061.
- Historical Python/Node reader/writer Rust-port work: superseded by SOW-0116
  for product planning.
- Remaining Netdata component integration/release work is tracked outside
  SOW-0009 by pending SOW-0048, SOW-0049, SOW-0050, and SOW-0066.
- No new SOW-0009 follow-up is required.

## Outcome

Completed. SOW-0009 served as the performance umbrella and all linked
implementation/performance work is now closed through focused child SOWs. This
closeout changes only SOW lifecycle and mapping records.

## Lessons Extracted

- Keep performance work split by hot path and contract.
- Keep umbrella SOW follow-up lists synchronized with later child SOWs.
- Close umbrella SOWs once child SOWs finish, instead of leaving stale paused
  state in `current/`.

## Followup

- SOW-0042 - Writer Final Certification.
- SOW-0043 - Rust Reader Libsystemd/Jf Parity.
- SOW-0044 - Rust Reader Hot-Path Optimization.
- SOW-0045 - Go Reader Alignment Optimization.
- SOW-0052 - Rust Reader Last-Mile Optimization.
- SOW-0056 - Go Reader Hot-Path Optimization Phase 2.
- SOW-0060 - Rust Reader Absolute Hot-Path Profiling.
- SOW-0061 - Cross-Language Row-Scoped Facade Lifetime.
- SOW-0062 - Rust And Go Writer Absolute Performance.

All SOW-0009 follow-up items above are completed.

## Regression Log

None yet.
