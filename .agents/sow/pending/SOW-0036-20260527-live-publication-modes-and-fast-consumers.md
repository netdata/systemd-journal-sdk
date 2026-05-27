# SOW-0036 - Live Publication Modes And Fast Consumers

## Status

Status: open

Sub-state: pending analysis and user decisions; no implementation started.

## Requirements

### Purpose

Give SDK consumers explicit control over the compatibility/performance tradeoff
for live-reader publication, while preserving systemd-compatible behavior by
default. The design must also identify similar configurable opportunities where
some consumers need stock external reader compatibility and others, such as
same-process Netdata ingestion paths, only need SDK-controlled readers.

### User Request

The user asked to create a SOW, not implement now, for making the new
post-change live-reader notification configurable. The user also asked whether
same-process reader/writer consumers, such as NetFlow, change the tradeoff, and
asked to identify similar optimization opportunities before implementation.

### Assistant Understanding

Facts:

- The Rust low-level writer now calls `journal_file.post_change()` after entry
  metadata publication.
- `post_change()` issues a sequentially consistent fence plus same-size
  truncate to trigger stock reader file-change notification after mmap writes.
- systemd v260.1 does the same in `journal_file_post_change()` and calls it
  after each append when no coalescing timer is installed.
- This notification is needed for stock `journalctl --follow` and stock
  libsystemd follow-style readers to wake promptly while a file is actively
  appended through mmap.
- The notification is not needed for closed-file byte identity, closed-file
  verification, or final reads after sync/close.
- Same-process consumers can use an SDK-controlled wake path instead of kernel
  file-change notification, provided no stock external reader compatibility is
  claimed for that mode.
- NetFlow is user-described as a same-process reader/writer consumer.
- The current performance evidence shows the Rust/systemd ratio dropped from
  about `1.56x` in the prior fixed-128 MiB run to about `1.50x` after adding
  Rust post-change notification. The post-change syscall has real overhead, but
  it did not erase the measured speedup.

Inferences:

- A single boolean may be sufficient for the immediate problem, but an enum-like
  policy is likely cleaner because systemd has immediate and coalesced
  notification behavior, while Netdata same-process paths may prefer disabled
  or manual notification.
- Same-process consumers still need a visibility/wakeup contract. Disabling
  stock notification is safe only if the reader is driven by the writer through
  an in-process signal, queue, cursor handoff, or explicit polling strategy.
- The option must be explicit in benchmark output and in live-compatibility
  claims. A disabled notification mode must not be reported as stock
  `journalctl --follow` compatible.

Unknowns:

- Whether the first implementation should expose only `enabled/disabled` or a
  richer policy with immediate, coalesced, manual, and disabled modes.
- Whether the high-level directory writer should default differently from the
  low-level file writer for Netdata-specific constructors.
- Whether same-process reader/writer optimizations should add a shared
  in-process notification primitive to the SDKs or remain a consumer-owned
  responsibility.
- Which additional compatibility/performance knobs are worth exposing as public
  API and which should remain internal benchmark-only controls.

### Acceptance Criteria

- Analyze live publication requirements for external stock readers, SDK
  cross-process readers, SDK same-process readers, and closed-file readers.
- Decide the public cross-language API shape for live publication policy.
- Decide defaults for low-level file writers and high-level directory writers.
- Decide what same-process consumers may disable without losing their claimed
  compatibility contract.
- Identify similar configurable compatibility/performance opportunities and
  classify each as public option, internal optimization, or rejected unsafe knob.
- Implement the selected policy consistently in Rust, Go, Node.js, and Python
  only after user decisions are recorded.
- Ensure benchmark output records publication policy and does not compare modes
  without labeling them.
- Ensure live compatibility tests require stock follow-reader success only for
  modes that claim stock live-reader compatibility.
- Ensure disabled or manual modes still pass closed-file verification and
  cross-language reads after sync/close.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `rust/src/crates/journal-core/src/file/writer.rs`
- `rust/src/crates/journal-core/src/file/file.rs`
- `rust/src/crates/journal-core/src/file/mmap.rs`
- `go/journal/writer.go`
- `.agents/sow/specs/product-scope.md`
- `tests/conformance/live/README.md`
- `tests/interoperability/README.md`
- `tests/benchmarks/run_writer_core_benchmarks.py`
- `tests/benchmarks/README.md`
- `tests/interoperability/run_journalctl_query_matrix.py`

Current state:

- Rust immediate post-change notification is now unconditional in the low-level
  writer path.
- Go immediate post-change notification is unconditional in the low-level writer
  path.
- Node.js and Python direct writers are still syscall/write based and do not
  currently have the same mmap post-change optimization surface, but their live
  behavior still needs a documented publication policy if this becomes
  cross-language API.
- Product scope currently treats live concurrency compatibility as mandatory
  for production-compatible writers. This SOW must refine that statement by
  distinguishing default compatibility mode from explicitly selected fast
  consumer modes.
- File-backed journalctl rewrites implement `--follow` by polling snapshots,
  not by requiring kernel inotify wakeups. That is a different reader behavior
  from stock `journalctl --file --follow`.

Risks:

- Making notification optional without strong naming can let users accidentally
  disable stock live-reader compatibility and still expect `journalctl --follow`
  to work.
- A boolean may be too vague: `false` could mean disabled, manually flushed, or
  coalesced elsewhere.
- A same-process reader optimization can accidentally become a hidden global
  behavior change if it is placed in generic constructors instead of
  Netdata-specific or explicit options.
- Coalescing improves throughput but changes maximum reader wake latency.
- Disabling notification improves writer throughput but external stock follow
  readers may lag until another file metadata event, explicit sync/close, or
  process exit.
- Overexposing unsafe knobs can make SDK compatibility claims impossible to
  reason about.

## Pre-Implementation Gate

Status: needs-user-decision

Problem / root-cause model:

- The SDK now has at least two valid consumer classes:
  stock-compatible live file producers and same-process SDK-controlled
  producers. The first needs systemd-style notification after mmap append. The
  second may not need kernel file-change notification and can avoid the
  per-entry syscall.
- The current low-level Rust and Go behavior optimizes for stock live-reader
  compatibility by default. This is the safest default but not always the best
  performance choice for same-process consumers.
- Similar opportunities probably exist wherever compatibility work is only
  needed for external tooling, while an SDK-controlled consumer can use a
  narrower contract.

Evidence reviewed:

- `rust/src/crates/journal-core/src/file/writer.rs`: low-level Rust writer calls
  `journal_file.post_change()` after `entry_added()`.
- `rust/src/crates/journal-core/src/file/mmap.rs`: post-change uses a
  sequentially consistent fence plus same-size `set_len`.
- `go/journal/writer.go`: Go writer calls `postChange()` after publishing entry
  metadata.
- `.agents/sow/specs/product-scope.md`: live concurrency compatibility is
  currently mandatory for production-compatible writers.
- `tests/conformance/live/README.md`: stock `journalctl --file --follow` and
  stock libsystemd readers are part of the live compatibility evidence.

Affected contracts and surfaces:

- Rust writer options and high-level log/directory writer options.
- Go writer options and high-level log/directory writer options.
- Node.js writer options and high-level directory writer options.
- Python writer options and high-level directory writer options.
- Benchmark report schema.
- Live compatibility tests.
- Product scope spec.
- README/API documentation for each language.
- Netdata integration guidance for NetFlow, SNMP traps, OTEL, and reader paths.

Existing patterns to reuse:

- Existing per-language writer option structs/config objects.
- Existing benchmark `api_mode`, `format`, `compression`, and `fss` reporting.
- Existing live matrix feature labeling.
- Existing systemd-compatible default policy in writer behavior.
- SOW-0023 API unification decisions for writer-facing Netdata integration.

Risk and blast radius:

- Public API change across four languages.
- Benchmark comparability risk if publication modes are not included in output.
- Live compatibility regression risk if default behavior changes.
- Netdata integration risk if fast same-process mode is selected for a path
  that later needs stock external follow readers.
- Documentation risk if disabled notification is described as generally safe
  instead of safe only for narrower consumer contracts.

Sensitive data handling plan:

- This SOW and its future work require only synthetic benchmark data,
  repository paths, and aggregate performance evidence. No raw secrets, SNMP
  communities, customer identifiers, private endpoints, or production logs
  should be written to durable artifacts.

Implementation plan:

1. Analyze and document the consumer classes and compatibility claims:
   stock external follow readers, SDK external readers, SDK same-process
   readers, and closed-file readers.
2. Inventory writer hot-path knobs that are candidates for public options:
   live publication notification, coalescing, sync/durability cadence, lock
   enforcement, validation/readback, metadata publication batching, and
   reader-wakeup strategy.
3. Present user decisions with concrete options and recommendation.
4. After decisions, update product-scope specs and per-language API docs.
5. Implement the selected live publication policy in Rust, Go, Node.js, and
   Python with default stock-compatible behavior.
6. Update benchmark output and tests so every result records publication policy.
7. Add or update live tests to prove stock follow compatibility only for modes
   that claim it, and closed-file compatibility for disabled/manual modes.
8. Run reviewers and validation before closing.

Validation plan:

- Unit tests for option parsing/defaults in every language.
- Writer-core benchmark in each selected publication mode.
- Live matrix proving immediate/coalesced modes wake stock readers according to
  their contract.
- Disabled/manual mode tests proving closed-file verify/read after sync/close.
- Same-process SDK reader/writer test if an SDK-provided same-process wake
  primitive is selected.
- Benchmark JSON schema check to require publication policy.
- External reviewer pass before close.
- `.agents/sow/audit.sh` before close.

Artifact impact plan:

- AGENTS.md: likely unaffected unless compatibility language needs a project-wide
  guardrail update.
- Runtime project skills: likely update `project-journal-compatibility` if the
  compatibility contract gains explicit publication modes.
- Specs: update `.agents/sow/specs/product-scope.md`.
- End-user/operator docs: update per-language README/API docs where options are
  described.
- End-user/operator skills: likely unaffected unless output/reference skills are
  later added.
- SOW lifecycle: this SOW remains pending until user decisions are made; it must
  not be implemented now.
- SOW-status.md: update project status summary to record this pending SOW.

Open-source reference evidence:

- No new external repository inspection was needed to create this SOW. systemd
  v260.1 evidence is already present in the repository-local source mirror and
  cited in active SOW-0009; future implementation should record durable
  `systemd/systemd @ <commit>` references if it reuses additional upstream
  behavior.

Open decisions:

1. Live publication API shape.
   - Option A: boolean `live_notifications: true|false`.
   - Option B: enum `immediate|disabled`.
   - Option C: enum `immediate|coalesced|manual|disabled`.
   - Recommendation: Option C. It names the operational contract clearly and
     leaves room for systemd-like coalescing without a later breaking change.

2. Defaults.
   - Option A: default `immediate` for all constructors.
   - Option B: default `immediate` for generic SDK constructors, allow
     Netdata-specific helpers to default to `manual` or `disabled`.
   - Option C: default `disabled` for performance.
   - Recommendation: Option B. Generic SDK behavior should stay stock-compatible
     by default, while Netdata hot paths can opt into narrower contracts.

3. Same-process reader/writer contract.
   - Option A: consumer-owned signaling; SDK only disables stock notification.
   - Option B: SDK-provided same-process notification primitive, such as a
     callback/channel/event counter, so readers can avoid filesystem wakeups.
   - Option C: no special same-process support; use coalesced notification only.
   - Recommendation: start with Option A unless NetFlow integration proves a
     reusable SDK primitive is needed. This keeps the SDK simpler and avoids
     designing a cross-language event abstraction prematurely.

4. Similar opportunities to expose.
   - Option A: only live publication notification in this SOW.
   - Option B: analyze all candidates but implement only live publication.
   - Option C: implement several knobs together.
   - Recommendation: Option B. The user explicitly asked to know if similar
     opportunities exist before implementation; broad analysis should happen,
     but implementation should stay focused unless a second knob is clearly
     valuable and safe.

## Implications And Decisions

Pending user decisions. No implementation may begin until the user selects the
policy shape, defaults, same-process contract, and candidate scope.

## Plan

1. Complete compatibility/performance option inventory and classify public vs
   internal knobs.
2. Present evidence-backed user decisions.
3. Implement only after decisions are recorded.
4. Validate across all languages and compatibility modes.
5. Update specs/docs and close with reviewer approval.

## Delegation Plan

Implementer:

- Local implementation only unless the user explicitly re-enables external
  implementer agents.

Reviewers:

- Use read-only reviewer agents from the approved pool after implementation.
  Skip `llm-netdata-cloud/mimo-v2.5-pro` while unavailable.

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

- Record reviewer or validation failures in this SOW.
- Do not close if live compatibility claims and benchmark mode labels disagree.
- Do not close if `.agents/sow/audit.sh` fails.

## Execution Log

### 2026-05-27

- Created this pending SOW at the user's request.
- Recorded that no implementation should happen yet.
- Recorded the same-process consumer implication: same-process SDK-controlled
  readers can avoid stock kernel wakeups if they use an explicit in-process
  wake/poll contract, but they must not claim stock external follow-reader
  compatibility in that mode.

## Validation

Acceptance criteria evidence:

- Pending.

Tests or equivalent validation:

- Pending.

Real-use evidence:

- Pending.

Reviewer findings:

- Pending.

Same-failure scan:

- Pending.

Sensitive data gate:

- This SOW contains only repository paths, synthetic benchmark context, and
  aggregate performance discussion. It does not contain raw secrets, SNMP
  communities, customer identifiers, personal data, private endpoints, or
  production log data.

Artifact maintenance gate:

- AGENTS.md: no update needed for SOW creation.
- Runtime project skills: no update needed until decisions change the durable
  compatibility workflow.
- Specs: no update yet; this SOW is pending decisions and implementation.
- End-user/operator docs: no update yet; no behavior changed.
- End-user/operator skills: no update needed.
- SOW lifecycle: pending SOW created with `Status: open`.
- SOW-status.md: should be updated with this pending SOW.

Specs update:

- Pending future implementation.

Project skills update:

- Pending future implementation if compatibility workflow changes.

End-user/operator docs update:

- Pending future implementation.

End-user/operator skills update:

- Not affected by SOW creation.

Lessons:

- Same-process consumers can legitimately use a narrower contract than stock
  external follow-reader compatibility, but that contract must be explicit in
  API names, benchmark output, and tests.

Follow-up mapping:

- Pending decisions and implementation.

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
