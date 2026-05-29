# SOW-0009 - Benchmark Profile Optimize

## Status

Status: paused

Sub-state: retained as the umbrella performance program. The writer side is
split into SOW-0042 after SOW-0037/SOW-0040/SOW-0041. The reader side is split
into SOW-0044, SOW-0052, SOW-0053, and SOW-0054 after SOW-0043 defines the
reader compatibility target and after the user changed priority to
Rust -> Python -> Node.js full-language ports. SOW-0060 completed the Rust
reader absolute hot-path profiling pass after later Go results set a stricter
performance bar.

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
- Reader performance work has not started yet.

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
- Whether Python and Node.js are expected to be production hot-path
  replacements or portability/reference implementations after measurement.

### Acceptance Criteria

- This umbrella SOW remains the performance program index until child SOWs
  close.
- Writer benchmarking and optimization are tracked by SOW-0042.
- Rust reader baseline/parity and optimization are tracked by SOW-0043,
  SOW-0044, SOW-0052, and SOW-0060.
- Go reader alignment and optimization are tracked by SOW-0045.
- Python reader/writer Rust-port work is tracked by SOW-0053.
- Node.js reader/writer Rust-port work is tracked by SOW-0054.
- Netdata integration remains blocked by the relevant child SOWs unless the
  user explicitly accepts a staged exception.
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

- Writer implementation is substantially improved by `v0.3.0`, based on the
  user-reported SNMP traps result.
- Controlled writer certification still needs to happen after writer closure
  SOWs.
- Reader optimization has not been started and needs a separate parity baseline
  first.

Risks:

- Running broad benchmarks before compatibility closure can produce numbers
  that become invalid after later API or format changes.
- Combining writer and reader optimization in one SOW makes it hard to isolate
  regressions.
- Netdata integration before reader performance could regress
  `systemd-journal.plugin`, `otel-signal-viewer.plugin`, or NetFlow query paths.

## Pre-Implementation Gate

Status: paused umbrella; child SOWs own implementation gates

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
- Rust, Go, Node.js, and Python SDK performance claims.
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

1. Keep this SOW paused as the performance index.
2. Complete writer closure and all-language writer parity SOWs.
3. Complete SOW-0042 for writer benchmark/certification.
4. Complete reader parity and reader performance SOWs.
5. Update this umbrella SOW only when child SOWs change the performance program.

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
- SOW lifecycle: this remains current/paused until the performance program is
  fully closed.
- SOW-status.md: updated by the restructuring commit.

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
7. Complete SOW-0053 Python reader/writer Rust-port work.
8. Complete SOW-0054 Node.js reader/writer Rust-port work.
9. Use the results to unblock SOW-0026 and component Netdata integration SOWs.

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
  established the Rust row-scoped current-entry facade payload lifetime. Go,
  Node.js, and Python facade parity for that strengthened lifetime remains
  tracked by this reader-performance umbrella before the reader phase can
  close.
- SOW-0061 completed Go, Node.js, and Python facade DATA enumeration parity for
  the Rust row-scoped current-entry payload lifetime contract. Public docs,
  product specs, and tests now record that cached current-row payload
  references/objects remain valid after end-of-row enumeration and until the
  next row/lifecycle boundary.

## Validation

Acceptance criteria evidence:

- Pending child SOW completion.

Tests or equivalent validation:

- SOW audit will validate the restructuring.

Real-use evidence:

- User reported SNMP traps improved from about 5.5k traps/s on `v0.1.0` to
  about 170k traps/s on `v0.3.0`; this remains contextual real-use evidence.

Reviewer findings:

- Pending child SOWs.

Same-failure scan:

- Pending child SOWs.

Sensitive data gate:

- This rescope records no raw secrets, credentials, bearer tokens, SNMP
  communities, customer names, personal data, non-private customer-identifying
  IPs, private endpoints, or proprietary incident details.

Artifact maintenance gate:

- AGENTS.md: no update needed for this rescope.
- Runtime project skills: no update needed for this rescope.
- Specs: no behavior change in this rescope.
- End-user/operator docs: no behavior change in this rescope.
- End-user/operator skills: no output/reference skill affected.
- SOW lifecycle: child SOWs created by the restructuring commit.
- SOW-status.md: updated by the restructuring commit.

Specs update:

- No spec update needed for the rescope itself.

Project skills update:

- No project skill update needed for the rescope itself.

End-user/operator docs update:

- No docs update needed for the rescope itself.

End-user/operator skills update:

- No output/reference skill update needed.

Lessons:

- Performance SOWs must be scoped by hot path and contract; a single broad
  benchmark SOW became too easy to misread.

Follow-up mapping:

- Writer certification: SOW-0042.
- Rust reader parity/performance: SOW-0043, SOW-0044, and SOW-0052.
- Go reader performance: SOW-0045.
- Python reader/writer Rust-port work: SOW-0053.
- Node.js reader/writer Rust-port work: SOW-0054.
- Cross-language row-scoped current-entry facade payload lifetime parity:
  completed by SOW-0061.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

- SOW-0042 - Writer Final Certification.
- SOW-0043 - Rust Reader Libsystemd/Jf Parity.
- SOW-0044 - Rust Reader Hot-Path Optimization.
- SOW-0045 - Go Reader Alignment Optimization.
- SOW-0052 - Rust Reader Last-Mile Optimization.
- SOW-0053 - Python Reader And Writer Rust Port.
- SOW-0054 - Node.js Reader And Writer Rust Port.

## Regression Log

None yet.
