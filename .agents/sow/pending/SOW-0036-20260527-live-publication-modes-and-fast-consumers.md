# SOW-0036 - Live Publication Modes And Fast Consumers

## Status

Status: open

Sub-state: pending analysis and user decisions; no implementation started.

## Requirements

### Purpose

Give SDK consumers explicit control over the compatibility/performance tradeoff
for live-reader publication, while preserving systemd-compatible behavior by
default. The design must also identify similar configurable opportunities where
some consumers need stock external follow-reader compatibility and others, such
as poll/snapshot-based Netdata UI paths, do not need true real-time visibility
of appended events during a read session.

### User Request

The user asked to create a SOW, not implement now, for making the new
post-change live-reader notification configurable. The user clarified that the
main distinction is not same-process versus cross-process, but whether the
consumer needs true real-time visibility of appended log events. Netdata UI is
poll based; if a file changes while a reader session is scanning, it is
acceptable for that reader session not to observe the newly appended lines. The
user asked to identify further optimization opportunities for same-process,
poll/snapshot, or latency-tolerant consumers.

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
- Poll/snapshot consumers do not need kernel wakeups for every append. They can
  discover new data on the next poll or explicit refresh.
- Same-process consumers are a subset of latency-tolerant consumers, but the
  stronger distinction is whether readers require live follow semantics or
  snapshot-at-open/session semantics.
- Netdata UI is user-described as poll based, and it is acceptable if an active
  reader session does not observe lines appended after that session began.
- The user agrees with the poll/snapshot direction but requires measuring the
  benefits before committing to any public API or production behavior.
- systemd v260.1 and the Rust SDK use windowed journal file mappings for normal
  reader/writer object access, while the Go writer currently mmaps the whole
  allocated journal file on Unix and remaps it when the file allocation grows.
- The Go reader currently uses `ReadAt()` into allocated buffers instead of mmap.
- systemd and Rust reader paths use mmap windows for journal object access. The
  Go reader's current `ReadAt()` design is a likely reader-throughput bottleneck
  for full scans, filtered scans, and directory reads, but the size of the
  impact must be measured.
- The current performance evidence shows the Rust/systemd ratio dropped from
  about `1.56x` in the prior fixed-128 MiB run to about `1.50x` after adding
  Rust post-change notification. The post-change syscall has real overhead, but
  it did not erase the measured speedup.

Inferences:

- A single boolean may be sufficient for the immediate problem, but an enum-like
  policy is likely cleaner because systemd has immediate and coalesced
  notification behavior, while Netdata same-process paths may prefer disabled
  or manual notification.
- Poll/snapshot consumers still need a visibility contract, but not necessarily
  per-entry visibility. Disabling stock notification is safe if the consumer
  accepts refresh/poll latency and readers snapshot file/header state at the
  start of each read session.
- The option must be explicit in benchmark output and in live-compatibility
  claims. A disabled notification mode must not be reported as stock
  `journalctl --follow` compatible.
- There are at least two separate optimization levers:
  - wakeup notification: the same-size truncate that wakes inotify/follow
    readers;
  - visibility publication: how often header/tail counters and indexes are made
    visible to newly opened snapshot readers.
- Public API should not be committed before measurement proves material benefit
  and compatibility risk is understood. Prototype controls should be
  benchmark-only or clearly experimental until then.

Unknowns:

- Whether the first implementation should expose only `enabled/disabled` or a
  richer policy with immediate, coalesced, manual, and disabled modes.
- Whether the high-level directory writer should default differently from the
  low-level file writer for Netdata-specific constructors.
- Whether poll/snapshot modes should only disable wakeup notification or also
  allow batched visibility publication.
- Whether same-process reader/writer optimizations should add shared in-process
  indexes/cursors or remain a consumer-owned responsibility.
- Which additional compatibility/performance knobs are worth exposing as public
  API and which should remain internal benchmark-only controls.
- Whether whole allocated-file mmap is faster enough than windowed mmap in hot
  writer paths to justify an explicit memory-for-throughput option. It will
  increase virtual memory pressure; the open question is whether it produces a
  material throughput win in Netdata-shaped workloads.
- Whether the Go reader should switch from `ReadAt()` buffers to mmap-backed
  reads, and if so whether the implementation should use systemd/Rust-style
  bounded windows or whole-file mapping for selected high-throughput modes.
- What "material benefit" threshold should justify a public option. Until the
  user decides otherwise, the recommended default is to require a measurable
  benefit large enough to matter to Netdata hot paths, not a micro-optimization
  that complicates the API.

### Acceptance Criteria

- Analyze live publication requirements for external stock follow readers,
  poll/snapshot readers, SDK same-process readers, SDK cross-process readers,
  and closed-file readers.
- Decide the public cross-language API shape for live publication policy.
- Decide defaults for low-level file writers and high-level directory writers.
- Decide what poll/snapshot consumers may disable or batch without losing their
  claimed compatibility contract.
- Identify similar configurable compatibility/performance opportunities and
  classify each as public option, internal optimization, or rejected unsafe knob.
- Measure each candidate opportunity with benchmark-only/internal switches
  before committing to a public API, documented default, or production contract.
- Record rows/sec, bytes/sec, CPU time, syscall counts, allocation behavior, and
  reader-visible semantics for each candidate mode.
- For writer mmap strategy candidates, also record mapped virtual size,
  resident memory behavior where practical, remap frequency, page faults, and
  throughput on fixed-128 MiB compact/no-compression/no-FSS writer workloads.
- For reader mmap strategy candidates, measure single-file full scan,
  single-file filtered scan, projected/export output, and ordered directory
  scans. Record rows/sec, bytes/sec, CPU, syscalls, allocations, mapped virtual
  size, resident memory behavior where practical, and page faults.
- Reject or keep internal any candidate whose measured benefit is too small for
  the added API/semantic complexity.
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
- A poll/snapshot optimization can accidentally become a hidden global behavior
  change if it is placed in generic constructors instead of explicit options.
- Coalescing improves throughput but changes maximum reader wake latency.
- Disabling notification improves writer throughput but external stock follow
  readers may lag until another file metadata event, explicit sync/close, or
  process exit.
- Batching visibility publication can improve throughput but newly opened poll
  readers may see data only up to the last publication boundary.
- Whole allocated-file mmap can reduce hot-path bounds/window lookup work and
  may improve maximum writer throughput, but it increases virtual address space
  usage and remaps the whole allocation on growth.
- mmap-backed readers can reduce read syscalls and allocations, but unsafe or
  overly broad mappings can make truncation/corruption handling harder. Go
  reader mmap work must prove it does not crash the process on truncated,
  rotated, or corrupt files covered by the project's reader contracts.
- Overexposing unsafe knobs can make SDK compatibility claims impossible to
  reason about.
- Prematurely committing public API options before measuring their benefit can
  leave permanent complexity that users gain little from.

## Pre-Implementation Gate

Status: needs-user-decision

Problem / root-cause model:

- The SDK now has at least two valid consumer classes:
  stock-compatible live file producers and poll/snapshot producers. The first
  needs systemd-style notification after mmap append. The second does not need
  kernel file-change notification and can avoid the per-entry syscall if it
  accepts refresh latency.
- The current low-level Rust and Go behavior optimizes for stock live-reader
  compatibility by default. This is the safest default but not always the best
  performance choice for Netdata poll/snapshot consumers.
- Similar opportunities probably exist wherever compatibility work is only
  needed for external follow tooling, while an SDK-controlled or poll/snapshot
  consumer can use a narrower contract.
- Writer mmap strategy is a separate performance axis from live notification:
  systemd/Rust-style windowed mmap minimizes mapped virtual memory, while Go's
  current whole allocated-file mmap may trade virtual memory pressure for fewer
  window lookups and faster direct writes.
- Reader mmap strategy is another separate axis. The current Go reader pays
  `ReadAt()` syscall and buffer-allocation costs through hot read paths, unlike
  systemd/Rust windowed mmap readers.
- The next step is not public API design; it is a measurement spike. Only
  options with material measured benefit and clear contracts should graduate to
  public API.

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
- `go/journal/writer.go`: Go currently publishes object metadata, entry
  metadata, and post-change notification on every append.
- `go/journal/mmap_unix.go`: Go writer maps offset `0` for the full allocated
  arena size and remaps after growth.
- `rust/src/crates/journal-core/src/file/mmap.rs`: Rust uses a `WindowManager`
  with bounded windows and maps requested offset ranges.
- `rust/src/crates/journal-core/src/file/file.rs`: Rust high-level journal file
  code documents the windowed mapping strategy and keeps separate persistent
  maps for header and hash tables.
- `go/journal/reader.go`: Go reader uses `ReadAt()` into buffers for headers,
  entry arrays, entries, and data payloads.
- `go/journal/reader.go`: hot reader paths allocate buffers before `ReadAt()`
  for entry arrays, entry headers/items, data headers, and data payloads.
- `rust/src/crates/journal-core/src/file/writer.rs`: Rust currently updates
  header/tail metadata on every append and calls post-change on every append.

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
- Netdata integration risk if a poll/snapshot mode is selected for a path that
  later needs stock external follow readers.
- Documentation risk if disabled notification is described as generally safe
  instead of safe only for narrower consumer contracts.
- API debt risk if options are added because they seem plausible but deliver
  negligible throughput or latency improvements in controlled benchmarks.
- Memory-behavior risk if a whole-file mmap option is generalized without clear
  workload guidance; it may be appropriate for maximum-throughput Netdata paths
  but inappropriate for constrained systems or many simultaneously open files.
- Reader-safety risk if Go mmap reading is implemented without systemd-like
  bounds discipline. The reader must preserve corrupt-file rejection, mixed
  directory behavior, and active-writer snapshot/live contracts.

Sensitive data handling plan:

- This SOW and its future work require only synthetic benchmark data,
  repository paths, and aggregate performance evidence. No raw secrets, SNMP
  communities, customer identifiers, private endpoints, or production logs
  should be written to durable artifacts.

Implementation plan:

1. Analyze and document the consumer classes and compatibility claims:
   stock external follow readers, poll/snapshot readers, SDK external readers,
   SDK same-process readers, and closed-file readers.
2. Inventory writer hot-path knobs that are candidates for public options:
   live wakeup notification, visibility publication cadence, reader snapshot
   mode, coalescing, sync/durability cadence, lock enforcement,
   validation/readback, metadata publication batching, hash-depth maintenance,
   raw/prepared field APIs, reader-wakeup strategy, writer mmap strategy
   (`windowed` versus `whole allocated file`), and reader mmap strategy
   (`ReadAt` buffers versus mmap-backed access).
3. Add benchmark-only/internal experimental switches for the smallest set of
   candidate modes needed to measure benefit. These switches must not be
   documented as public API.
4. Measure each candidate against the stock-compatible baseline and record the
   semantic contract each candidate preserves or drops.
5. Present evidence-backed user decisions with benchmark numbers and risk
   analysis.
6. After decisions, update product-scope specs and per-language API docs.
7. Implement the selected public policy consistently in Rust, Go, Node.js, and
   Python with default stock-compatible behavior.
8. Update benchmark output and tests so every result records publication policy.
9. Add or update live tests to prove stock follow compatibility only for modes
   that claim it, and closed-file compatibility for disabled/manual modes.
10. Run reviewers and validation before closing.

Validation plan:

- Unit tests for option parsing/defaults in every language.
- Writer-core benchmark in each experimental candidate mode before any public
  API commitment.
- Syscall and allocation profiles for each candidate mode, including at least
  wakeup notification disabled, snapshot reader mode, and any batched
  visibility publication prototype that is simple enough to measure safely.
- Writer mmap strategy profile comparing windowed mmap and whole allocated-file
  mmap, starting with compact/no-compression/no-FSS fixed-128 MiB writer
  workloads. The result must include throughput and memory behavior, not only
  rows/sec.
- Reader strategy profile comparing current Go `ReadAt()` buffers against an
  experimental mmap-backed reader. The comparison must cover both single-file
  and directory readers, because directory ordering and multi-file scans are
  required SDK behavior.
- Reader mmap safety validation must include truncated/corrupt fixture cases
  and active-writer snapshot/live cases before any mmap reader mode is accepted
  as production-grade.
- Live matrix proving immediate/coalesced modes wake stock readers according to
  their contract.
- Disabled/manual mode tests proving closed-file verify/read after sync/close.
- Same-process SDK reader/writer test if an SDK-provided same-process wake
  primitive is selected.
- Poll/snapshot reader tests proving an active reader session can ignore
  appends after its snapshot boundary while the next session sees data at the
  selected publication boundary.
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
   - Option A: treat same-process as a special case of poll/snapshot mode;
     consumer-owned signaling or polling decides when to refresh.
   - Option B: SDK-provided same-process notification primitive, such as a
     callback/channel/event counter, so readers can avoid filesystem wakeups.
   - Option C: no special same-process support; use coalesced stock
     notification only.
   - Recommendation: Option A. The user's clarification means the real contract
     is snapshot/poll tolerance, not process locality. Same-process primitives
     can be revisited only if NetFlow integration proves they remove a measured
     bottleneck.

4. Similar opportunities to expose.
   - Option A: only live publication notification in this SOW.
   - Option B: analyze all candidates but implement only the safest
     publication-mode controls first.
   - Option C: implement several knobs together.
   - Recommendation: Option B. The user explicitly asked to know if similar
     opportunities exist before implementation; broad analysis should happen,
     but implementation should stay focused unless a second knob is clearly
     valuable and safe.

5. Snapshot/poll visibility policy.
   - Option A: disable wakeup notification only; keep per-entry header/index
     publication.
   - Option B: disable wakeup notification and allow batched visibility
     publication by time, entry count, or explicit flush.
   - Option C: defer all visibility until sync/close.
   - Recommendation: start with Option A for the first implementation because
     it is low risk and preserves next-poll visibility. Analyze Option B with
     benchmarks because it may provide larger gains for Node.js/Python and some
     Go paths. Option C is only safe for consumers that do not expect active
     files to be readable during writing.

6. Measurement-before-commitment gate.
   - Option A: decide API first, then benchmark implementation details.
   - Option B: benchmark with internal/experimental switches first, then decide
     which modes deserve public API.
   - Option C: do not add any public controls; keep all optimizations internal.
   - Decision: Option B. The user explicitly requires measuring the benefits
     before committing to the options. This SOW must not graduate any mode to
     stable public API until benchmark evidence and semantic tradeoffs are
     recorded.

7. Writer mmap strategy candidate.
   - Option A: keep systemd/Rust-style windowed mmap only.
   - Option B: benchmark both windowed mmap and whole allocated-file mmap, then
     expose an option only if whole-file mmap proves materially faster for
     maximum-throughput writer workloads.
   - Option C: make whole allocated-file mmap the default writer strategy.
   - Recommendation: Option B. Whole-file mmap clearly costs more virtual memory
     pressure, but Netdata paths such as NetFlow may accept that cost if it
     buys meaningful throughput. The SOW must measure the benefit before
     deciding whether this becomes public API.

8. Reader mmap strategy candidate.
   - Option A: keep Go reader `ReadAt()` buffers and optimize allocations
     locally.
   - Option B: benchmark an experimental Go mmap-backed reader against the
     current `ReadAt()` reader, then decide if mmap should become the default
     or an optional high-throughput mode.
   - Option C: immediately rewrite the Go reader to mmap-backed access.
   - Recommendation: Option B. The current Go reader is very likely slower than
     an mmap-backed design for scan-heavy workloads, but the project needs
     measured throughput, allocation, syscall, and safety evidence before
     changing a reader contract used by conformance, journalctl, and Netdata
     integrations.

## Implications And Decisions

Pending user decisions. No implementation may begin until the user selects the
policy shape, defaults, poll/snapshot contract, candidate scope, and measured
benefit threshold. Benchmark-only/internal experiments are allowed only after
this SOW is activated and must not be documented as public API.

## Plan

1. Complete compatibility/performance option inventory and classify candidate
   knobs for measurement.
2. Add benchmark-only/internal experimental controls for the candidates selected
   for measurement.
3. Measure benefit and semantic loss for each candidate.
4. Present evidence-backed user decisions.
5. Implement public API only after decisions are recorded.
6. Validate across all languages and compatibility modes.
7. Update specs/docs and close with reviewer approval.

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
- Recorded the initial same-process consumer implication.
- Updated the SOW after the user clarified that the primary distinction is
  poll/snapshot tolerance, not process locality. Netdata UI polling does not
  need kernel wakeups, and an active reader session may ignore appends that
  happen after the session starts.
- Recorded the user's requirement that candidate controls must be measured
  before committing to them as public API or production behavior.
- Recorded writer mmap strategy as a concrete benchmark candidate after
  verifying that Rust/systemd use windowed mappings while the Go writer maps
  the whole allocated file on Unix.
- Recorded Go reader mmap strategy as a concrete benchmark candidate after the
  user challenged the asymmetry between Go's mmap writer and non-mmap reader.

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

- Poll/snapshot consumers can legitimately use a narrower contract than stock
  external follow-reader compatibility, but that contract must be explicit in
  API names, benchmark output, and tests.
- Plausible optimization knobs are not automatically worth public API. They
  must first prove a material benefit in controlled benchmarks.
- Whole allocated-file mmap is a legitimate memory-for-throughput hypothesis,
  not a default policy. It needs direct benchmark and memory-behavior evidence
  before it can become an SDK option.
- The Go reader's `ReadAt()` implementation is a likely performance issue for
  scan-heavy workloads, but mmap reader work must prove both speed and safety.

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
