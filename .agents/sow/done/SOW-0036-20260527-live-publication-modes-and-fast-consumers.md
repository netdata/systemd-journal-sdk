# SOW-0036 - Live Publication Modes And Fast Consumers

## Status

Status: completed

Sub-state: completed on 2026-05-28. The first phase was a Rust measurement
spike. After benchmark evidence and the user's 2026-05-28 decision, the SOW
scope graduated to a cross-language public `live_publish_every_entries` API.
SOW-0037 remains paused until the next explicit resume step.

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

On 2026-05-28, the user directed implementation to proceed with these Rust
experiments before moving to Go:

- implement experimental Rust options for live publication and writer mmap
  strategy;
- compare Rust with and without those options so the project can decide which
  ones are worth keeping;
- keep this as measurement work first, not a public cross-language API
  commitment;
- do not continue Go work until the Rust option evidence is available.

Later on 2026-05-28, after the Rust measurements were available, the user chose
the numeric public API shape: `1` for default systemd-compatible publication
after every entry, `0` for disabled explicit publication, and `N > 1` for
publication after every `N` entries. This setting is a live-reader publication
cadence, not durability sync.

After seeing the initial unpinned Go writer benchmark results, the user
clarified an additional performance requirement: do not make Go worse. Instead,
verify whether the apparent Go lead is real, identify the cause if confirmed,
and port any useful compatible fast path to Rust.

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
  and compatibility risk is understood. That gate is now satisfied for the live
  publication cadence; whole-file mmap remains rejected for now because it did
  not improve the measured workload.

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
- Why the initial unpinned Go default writer throughput measured higher than
  Rust default throughput on the writer-core benchmark. Later controlled pinned
  evidence did not reproduce the Go lead, so this remains an apparent benchmark
  protocol difference rather than a confirmed implementation advantage.

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
- Preserve the current Go writer benchmark performance while investigating and
  improving Rust. Go must not be made worse to make cross-language numbers look
  aligned.
- Explain the apparent Go-vs-Rust default writer gap with controlled evidence
  before claiming a Rust optimization is complete.
- Port any compatible Go-derived fast path to Rust as an explicit default-safe
  internal optimization or measured option, without weakening the
  systemd-compatible default.
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

Status: ready for measured cross-language public live publication API

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
- The live publication cadence has graduated from measurement spike to public
  API after the user chose the numeric contract. Other optimization levers
  remain measurement-only until separate evidence and decisions exist.

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

- This SOW requires only synthetic benchmark data,
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
- SOW lifecycle: this SOW records the user decisions, implementation,
  validation, review disposition, and completion move.
- SOW-status.md: update project status summary when this SOW closes.

Open-source reference evidence:

- No new external repository inspection was needed for this SOW. systemd v260.1
  behavior used here was already captured by prior compatibility work and by
  repository-local tests/specs.

Open decisions:

1. Live publication API shape.
   - Option A: boolean `live_notifications: true|false`.
   - Option B: numeric `live_publish_every_entries`.
     - `1`: publish after every entry; default systemd-compatible live-follow
       behavior.
     - `0`: disable per-entry live publication; fastest; stock
       live-follow/readers are not guaranteed to see appended entries until
       explicit sync/close or another file event.
     - `N > 1`: publish after every `N` entries; bounded wakeup latency and a
       measured compromise mode.
   - Option C: enum `immediate|coalesced|manual|disabled`.
   - Decision: Option B. The user chose the numeric cross-language contract on
     2026-05-28 after reviewing benchmark evidence. The name must avoid
     `sync` because this is reader-visibility/live-follow publication via
     same-size truncate, not disk durability or `fsync`.
   - Implementation note: the public API name is `live_publish_every_entries`
     with idiomatic language casing where required.

2. Defaults.
   - Option A: default `immediate` for all constructors.
   - Option B: default `immediate` for generic SDK constructors, allow
     Netdata-specific helpers to default to `manual` or `disabled`.
   - Option C: default `disabled` for performance.
   - Decision: Option A for generic SDK constructors and current high-level
     directory writers. Generic SDK behavior stays stock-compatible by default.
     Netdata hot paths can opt into narrower contracts explicitly by setting
     `live_publish_every_entries`.

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
   - Decision: Option B. The SOW analyzed adjacent opportunities, but
     implementation stayed focused on the selected live publication cadence.
     Reader mmap and broader performance work remain in SOW-0037/SOW-0009.

5. Snapshot/poll visibility policy.
   - Option A: disable wakeup notification only; keep per-entry header/index
     publication.
   - Option B: disable wakeup notification and allow batched visibility
     publication by time, entry count, or explicit flush.
   - Option C: defer all visibility until sync/close.
   - Decision: Option A for this SOW. The selected public option controls the
     explicit live-publication operation while preserving per-entry journal
     metadata publication. Option B/C remain out of scope until broader
     performance work measures them.

6. Measurement-before-commitment gate.
   - Option A: decide API first, then benchmark implementation details.
   - Option B: benchmark with internal/experimental switches first, then decide
     which modes deserve public API.
   - Option C: do not add any public controls; keep all optimizations internal.
   - Decision: Option B. The user explicitly required measuring the benefits
     before committing to the options. The live publication cadence now has
     recorded benchmark evidence and semantic tradeoffs, so it is allowed to
     graduate to public API. Other knobs remain measurement-only.

7. Writer mmap strategy candidate.
   - Option A: keep systemd/Rust-style windowed mmap only.
   - Option B: benchmark both windowed mmap and whole allocated-file mmap, then
     expose an option only if whole-file mmap proves materially faster for
     maximum-throughput writer workloads.
   - Option C: make whole allocated-file mmap the default writer strategy.
   - Decision: Option B was measured and rejected for this workload. Whole-file
     mmap did not improve Rust writer throughput enough to justify a public API
     or default change.

8. Reader mmap strategy candidate.
   - Option A: keep Go reader `ReadAt()` buffers and optimize allocations
     locally.
   - Option B: benchmark an experimental Go mmap-backed reader against the
     current `ReadAt()` reader, then decide if mmap should become the default
     or an optional high-throughput mode.
   - Option C: immediately rewrite the Go reader to mmap-backed access.
   - Decision: out of scope for this SOW and tracked by the reference-drift and
     broad performance work. No reader contract changed here.

9. Rust-first implementation order.
   - Decision: Rust live publication and writer mmap strategy experiments run
     first. Go work resumes only after the Rust baseline and option tradeoffs
     are measured.

## Implications And Decisions

Decision recorded on 2026-05-28:

- Rust benchmark/internal controls may be implemented now for live publication
  and writer mmap strategy.
- The first comparison must keep the production-shaped writer workload fixed:
  compact format, no compression, no FSS, and fixed `128 MiB` max file size.
- The default remains stock-compatible unless a later user decision changes it.
- Experimental switches must be labeled in benchmark output and must not be
  presented as stable public API.
- After the measurement evidence was recorded, the user chose the numeric live
  publication cadence API. Rust, Go, Node.js, and Python implementation changes
  are now in scope for this SOW. Whole-file mmap remains an internal/rejected
  experiment for now because it was slower than windowed mmap in every measured
  mode.

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

### 2026-05-28

- Activated this SOW for a Rust-only measurement spike and paused SOW-0037.
- Implemented Rust benchmark/internal controls only:
  - `ExperimentalLivePublicationMode::Immediate` keeps the current
    stock-compatible per-entry post-change notification.
  - `ExperimentalLivePublicationMode::Disabled` skips per-entry post-change and
    relies on final sync/close for closed-file visibility.
  - `ExperimentalLivePublicationMode::EveryN(n)` publishes post-change after
    every `n` appended entries.
  - `ExperimentalMmapStrategy::Windowed` keeps the current Rust/systemd-style
    bounded-window writer mapping.
  - `ExperimentalMmapStrategy::WholeFile` maps the whole currently allocated
    journal file for writer-owned files and remaps on allocation growth.
- Added benchmark labels and metrics for Rust `live_publication`,
  `mmap_strategy`, mmap map/remap/eviction counters, mapped bytes,
  `/proc/self/status` memory snapshots, and `/usr/bin/time` page-fault/context
  switch counters.
- Added a Rust unit test proving the publication modes preserve identical
  closed-file bytes after sync/close.
- Updated the benchmark README so future benchmark reports are not interpreted
  without publication/mmap labels.

Rust compact/no-compression/no-FSS fixed-128 MiB writer-core benchmark:

| Writer | Live publication | Mmap strategy | Median append rows/sec | Journal-size MiB/sec | Ratio vs systemd | Ratio vs Rust immediate/windowed |
|---|---:|---:|---:|---:|---:|---:|
| systemd | stock | n/a | `35838.270` | `45.87` | `1.000x` | `0.751x` |
| Rust | immediate | windowed | `47691.170` | `61.04` | `1.331x` | `1.000x` |
| Rust | disabled | windowed | `53872.273` | `68.96` | `1.503x` | `1.130x` |
| Rust | every-n:64 | windowed | `51975.923` | `66.53` | `1.450x` | `1.090x` |
| Rust | immediate | whole-file | `42343.867` | `54.20` | `1.182x` | `0.888x` |
| Rust | disabled | whole-file | `48418.460` | `61.98` | `1.351x` | `1.015x` |
| Rust | every-n:64 | whole-file | `46010.835` | `58.89` | `1.284x` | `0.965x` |

Benchmark evidence:

- Baseline systemd plus Rust immediate/windowed:
  `.local/benchmarks/sow36-rust-options/compact-none-fss-off-rust-structured-field-trusted-unique-live-immediate-mmap-windowed-20260528T042737168880Z/report.json`.
- Rust disabled/windowed:
  `.local/benchmarks/sow36-rust-options/compact-none-fss-off-rust-structured-field-trusted-unique-live-disabled-mmap-windowed-20260528T042846653145Z/report.json`.
- Rust every-64/windowed:
  `.local/benchmarks/sow36-rust-options/compact-none-fss-off-rust-structured-field-trusted-unique-live-every-n-every-64-mmap-windowed-20260528T042916703846Z/report.json`.
- Rust immediate/whole-file:
  `.local/benchmarks/sow36-rust-options/compact-none-fss-off-rust-structured-field-trusted-unique-live-immediate-mmap-whole-file-20260528T042942313724Z/report.json`.
- Rust disabled/whole-file:
  `.local/benchmarks/sow36-rust-options/compact-none-fss-off-rust-structured-field-trusted-unique-live-disabled-mmap-whole-file-20260528T043011122550Z/report.json`.
- Rust every-64/whole-file:
  `.local/benchmarks/sow36-rust-options/compact-none-fss-off-rust-structured-field-trusted-unique-live-every-n-every-64-mmap-whole-file-20260528T043036964419Z/report.json`.

Memory and mmap evidence from the 100k-row reports:

- Windowed modes: median `max_rss_kb` about `729900`, median minor page faults
  about `216800`, median `max_mapped_bytes` `184549376`, `map_count` `41`,
  `remap_count` `27`, `eviction_count` `0`.
- Whole-file modes: median `max_rss_kb` about `692400`, median minor page
  faults about `323279`, median `max_mapped_bytes` `134217728`, `map_count`
  `17`, `remap_count` `15`, `eviction_count` `0`.
- Fact: whole-file mmap reduced mapped virtual bytes and map/remap counts in
  this workload, but it was slower and incurred substantially more minor page
  faults.

Syscall evidence from 1000-row `strace -f -c` runs:

- Rust immediate/windowed: `1003` `ftruncate` calls.
- Rust disabled/windowed: `3` `ftruncate` calls.
- Rust every-64/windowed: `18` `ftruncate` calls.
- Rust immediate/whole-file: `1003` `ftruncate` calls.
- Result files:
  `.local/benchmarks/sow36-rust-options/strace/rust-immediate-windowed-1000.txt`,
  `.local/benchmarks/sow36-rust-options/strace/rust-disabled-windowed-1000.txt`,
  `.local/benchmarks/sow36-rust-options/strace/rust-every64-windowed-1000.txt`,
  `.local/benchmarks/sow36-rust-options/strace/rust-immediate-whole-1000.txt`.

Interpretation:

- Fact: disabling per-entry stock live notification improved Rust windowed
  append throughput by about `13.0%` on the fixed-128 MiB compact workload.
- Fact: publishing every 64 entries improved Rust windowed append throughput by
  about `9.0%`.
- Fact: whole-file mmap was slower than windowed mmap in every matching
  publication mode measured here.
- Current recommendation for the user decision: keep `windowed` as the writer
  mmap strategy. Keep `immediate` as the default stock-compatible publication
  mode. Consider graduating a live publication policy option because
  `disabled` and `every-n` provide material measured benefit for
  latency-tolerant consumers, but only with explicit compatibility labels.

No-sandbox rerun:

- On 2026-05-28 the user asked to rerun the same benchmark matrix outside the
  filesystem sandbox because the sandbox may affect measurements. The rerun
  used the same workload: `100000` rows, `32` fields per row, compact format,
  no compression, no FSS, fixed `134217728` byte max file size, one warmup,
  and three measured repetitions.

| Writer | Live publication | Mmap strategy | Median append rows/sec | Journal-size MiB/sec | Ratio vs systemd | Ratio vs Rust immediate/windowed |
|---|---:|---:|---:|---:|---:|---:|
| systemd | stock | n/a | `36457.998` | `46.67` | `1.000x` | `0.757x` |
| Rust | immediate | windowed | `48167.678` | `61.65` | `1.321x` | `1.000x` |
| Rust | disabled | windowed | `53949.544` | `69.06` | `1.480x` | `1.120x` |
| Rust | every-n:64 | windowed | `52635.177` | `67.37` | `1.444x` | `1.093x` |
| Rust | immediate | whole-file | `42323.019` | `54.17` | `1.161x` | `0.879x` |
| Rust | disabled | whole-file | `49455.369` | `63.30` | `1.357x` | `1.027x` |
| Rust | every-n:64 | whole-file | `48611.688` | `62.22` | `1.333x` | `1.009x` |

No-sandbox benchmark evidence:

- Baseline systemd plus Rust immediate/windowed:
  `.local/benchmarks/sow36-rust-options-nosandbox/compact-none-fss-off-rust-structured-field-trusted-unique-live-immediate-mmap-windowed-20260528T045248435885Z/report.json`.
- Rust disabled/windowed:
  `.local/benchmarks/sow36-rust-options-nosandbox/compact-none-fss-off-rust-structured-field-trusted-unique-live-disabled-mmap-windowed-20260528T045348774218Z/report.json`.
- Rust every-64/windowed:
  `.local/benchmarks/sow36-rust-options-nosandbox/compact-none-fss-off-rust-structured-field-trusted-unique-live-every-n-every-64-mmap-windowed-20260528T045409191286Z/report.json`.
- Rust immediate/whole-file:
  `.local/benchmarks/sow36-rust-options-nosandbox/compact-none-fss-off-rust-structured-field-trusted-unique-live-immediate-mmap-whole-file-20260528T045430467038Z/report.json`.
- Rust disabled/whole-file:
  `.local/benchmarks/sow36-rust-options-nosandbox/compact-none-fss-off-rust-structured-field-trusted-unique-live-disabled-mmap-whole-file-20260528T045453221161Z/report.json`.
- Rust every-64/whole-file:
  `.local/benchmarks/sow36-rust-options-nosandbox/compact-none-fss-off-rust-structured-field-trusted-unique-live-every-n-every-64-mmap-whole-file-20260528T045514466853Z/report.json`.

No-sandbox memory and mmap evidence from the 100k-row reports:

- Windowed modes: median `max_rss_kb` about `730000`, median minor page faults
  about `217000`, median `max_mapped_bytes` `184549376`, `map_count` `41`,
  `remap_count` `27`, `eviction_count` `0`.
- Whole-file modes: median `max_rss_kb` about `692000`, median minor page
  faults about `323700`, median `max_mapped_bytes` `134217728`, `map_count`
  `17`, `remap_count` `15`, `eviction_count` `0`.

No-sandbox syscall evidence from 1000-row `strace -f -c` runs:

- Rust immediate/windowed: `1003` `ftruncate` calls.
- Rust disabled/windowed: `3` `ftruncate` calls.
- Rust every-64/windowed: `18` `ftruncate` calls.
- Rust immediate/whole-file: `1003` `ftruncate` calls.
- Rust disabled/whole-file: `3` `ftruncate` calls.
- Rust every-64/whole-file: `18` `ftruncate` calls.
- Result files:
  `.local/benchmarks/sow36-rust-options-nosandbox/strace-20260528T045651Z/`.

No-sandbox interpretation:

- Fact: the non-sandbox matrix preserved the same ordering as the sandboxed
  matrix. The fastest measured mode is still Rust disabled/windowed.
- Fact: whole-file mmap remained slower than windowed mmap in every matched
  publication mode.
- Fact: the per-entry live publication cost remains visible outside the
  sandbox: disabled/windowed measured about `12.0%` faster than
  immediate/windowed.

Cross-language implementation after user decision B:

- Rust:
  - `JournalWriter::set_live_publish_every_entries()` and
    `JournalWriter::live_publish_every_entries()` expose the cadence.
  - `journal::Config::with_live_publish_every_entries()` propagates the same
    setting through high-level directory writers.
  - The writer-core benchmark accepts `--live-publish-every-entries` and keeps
    the previous mode flags as hidden compatibility aliases.
- Go:
  - `journal.Options.LivePublishEveryEntries` is a `*uint64` so `nil` preserves
    the default `1` while an explicit `0` disables explicit publication.
  - `journal.PublishEveryEntries()` is the public helper for setting the
    pointer value.
  - `OpenWithOptions()` lets reopen/append paths preserve the cadence.
- Node.js:
  - Direct and directory writers accept `livePublishEveryEntries` and
    `live_publish_every_entries`.
  - The option controls explicit same-size truncate publication calls. Ordinary
    write syscalls may still produce kernel-visible write events.
- Python:
  - Direct and directory writers accept `live_publish_every_entries` and
    `livePublishEveryEntries`.
  - The option controls explicit same-size truncate publication calls. Ordinary
    write syscalls may still produce kernel-visible write events.
- Shared benchmark harness:
  - `--live-publish-every-entries` is now the shared SDK benchmark option.
  - Benchmark reports include `live_publish_every_entries` in parameters and
    per-driver summaries.

Go no-sandbox writer benchmark after user request:

- On 2026-05-28 the user asked to prioritize Go benchmark results. The benchmark
  used the same production-shaped writer-core workload: `100000` rows, `32`
  fields per row, compact format, no compression, no FSS, fixed `134217728`
  byte max file size, one warmup, three measured repetitions, and no filesystem
  sandbox.
- The timed metric is append-loop time only; it excludes row generation, writer
  creation, final close/sync, and journal verification.

| Writer | `live_publish_every_entries` | Median append rows/sec | Journal-size MiB/sec | Ratio vs systemd | Ratio vs Rust default |
|---|---:|---:|---:|---:|---:|
| systemd | stock | `36597.745` | `46.85` | `1.000x` | `0.772x` |
| Rust | `1` | `47414.018` | `60.69` | `1.296x` | `1.000x` |
| Go | `1` | `50889.504` | `65.14` | `1.391x` | `1.073x` |
| Go | `64` | `56989.201` | `72.95` | `1.557x` | `1.202x` |
| Go | `0` | `60279.339` | `77.16` | `1.647x` | `1.271x` |

Benchmark evidence:

- Default systemd/Rust/Go:
  `.local/benchmarks/sow36-go-cadence-nosandbox/compact-none-fss-off-rust-structured-field-trusted-unique-live-every-1-mmap-windowed-20260528T053616612866Z/report.json`.
- Go every-64:
  `.local/benchmarks/sow36-go-cadence-nosandbox/compact-none-fss-off-20260528T053722833852Z/report.json`.
- Go disabled explicit publication:
  `.local/benchmarks/sow36-go-cadence-nosandbox/compact-none-fss-off-20260528T053742654453Z/report.json`.

Interpretation:

- Fact: the initial unpinned Go default `1` run measured faster than both
  systemd and Rust default in this writer-core workload.
- Fact: Go `64` improved median append throughput by about `12.0%` over Go
  default `1`.
- Fact: Go `0` improved median append throughput by about `18.5%` over Go
  default `1`.
- Fact: Go `0` is the fastest measured Go mode, but it is the narrowed
  latency-tolerant contract and must not be presented as stock follow-reader
  compatible without exact-mode live matrix evidence.
- Follow-up evidence below supersedes the initial unpinned Go-vs-Rust
  interpretation for cross-language ranking.

Go-vs-Rust controlled follow-up after the user asked not to make Go worse:

- The goal was to verify whether Go was truly faster and, if so, port the
  compatible fast path to Rust without reducing Go performance.
- Rust raw-payload vs structured-field comparison, no sandbox, compact,
  no-compression, no-FSS, fixed `134217728` byte max file size,
  `live_publish_every_entries=1`, `100000` rows:
  - Rust structured-field median append throughput:
    `44542.289 rows/sec`.
  - Rust raw-payload median append throughput:
    `43172.618 rows/sec`.
  - Go field API median append throughput in the same first run:
    `43604.817 rows/sec`.
  - Result: the structured Rust API wrapper path was not the reason for the
    initial Go lead; structured was slightly faster than raw in this run.
- Syscall comparison, `5000` rows, default live publication:
  - Rust windowed: `5004` `ftruncate` calls.
  - Rust whole-file: `5004` `ftruncate` calls.
  - Go: `5003` `ftruncate` calls.
  - Result: Go was not faster because it avoided per-entry live publication;
    both default writers perform the same class of publication syscall.
- Rust recent-DATA-cache size hypothesis:
  - Evidence: Go uses `65536` recent DATA cache slots while Rust uses `4096`.
  - Test: temporarily changed Rust to `65536` slots and reran the same
    no-sandbox benchmark against Go.
  - Result with Rust `65536` slots: Rust median `45392.651 rows/sec`, Go
    median `45830.637 rows/sec`.
  - A/B result with Rust restored to `4096` slots: Rust median
    `45644.153 rows/sec`.
  - Conclusion: the cache-size hypothesis was not confirmed, so the temporary
    Rust change was not kept.
- Controlled pinned benchmark:
  - Method: direct release benchmark binaries pinned with `taskset -c 3`,
    alternating execution order, compact/no-compression/no-FSS,
    fixed `134217728` byte max file size, `100000` rows, Rust
    structured-field API with trusted unique payloads, Go field API.
  - Default `live_publish_every_entries=1`:
    - Rust median: `43922.493 rows/sec`.
    - Go median: `43559.652 rows/sec`.
  - `live_publish_every_entries=64`:
    - Rust median: `47092.770 rows/sec`.
    - Go median: `46504.753 rows/sec`.
  - `live_publish_every_entries=0`:
    - Rust median: `50159.448 rows/sec`.
    - Go median: `48853.103 rows/sec`.
  - Result: controlled pinned runs did not reproduce the initial claim that Go
    is faster. Rust was slightly faster in all three pinned comparisons.
  - Interpretation: this does not prove a Go regression. The initial Go-only
    no-sandbox harness run and the later pinned alternating direct-binary run
    are different benchmark methods. They are evidence that the earlier Go lead
    was not stable enough to use as a Rust optimization target, not evidence
    that the Go implementation was made slower.
- Benchmark evidence:
  - Raw vs structured:
    `.local/benchmarks/sow36-rust-raw-vs-structured-nosandbox/`.
  - Syscall comparison:
    `.local/benchmarks/sow36-go-rust-strace-20260528T060239Z/`.
  - Cache-size experiment:
    `.local/benchmarks/sow36-rust-cache65536-nosandbox/` and
    `.local/benchmarks/sow36-rust-cache-ab-4096-nosandbox/`.
  - Pinned default comparison:
    `.local/benchmarks/sow36-rust-go-pinned-nosandbox-20260528T060632Z/`.
  - Pinned live-mode comparisons:
    `.local/benchmarks/sow36-rust-go-pinned-live-modes-20260528T060808Z/`.
- Conclusion: there is currently no proven Go-only fast path to port to Rust.
  The correct action is to preserve the current Go implementation, keep Rust
  windowed mmap as the default, keep the public live-publication cadence, and
  avoid adding Rust cache or mmap changes that are not supported by controlled
  evidence.

## Validation

Acceptance criteria evidence:

- Rust benchmark/internal controls were implemented and measured before any
  public API commitment.
- The user chose the numeric public API after seeing the measured tradeoff.
- Rust, Go, Node.js, and Python now expose the selected live publication
  cadence while preserving stock-compatible default `1`.
- Benchmark JSON now records `live_publish_every_entries`; Rust benchmark JSON
  also records the mmap strategy label used by the rejected/retained mmap
  experiment.
- Disabled and every-n modes pass closed-file stock verification in the
  benchmark runs after sync/close.
- Default immediate mode still passes the Rust live interoperability matrix.
- Controlled pinned Go-vs-Rust benchmarks did not reproduce the initial
  unpinned Go speed lead. No unproven Go-derived change was accepted into Rust,
  and Go was not changed during this follow-up. This is a benchmark-protocol
  correction, not a claimed Go performance regression.
- Product scope, project compatibility skill, benchmark docs, and language docs
  now describe the public cadence contract and the non-durability distinction.

Tests or equivalent validation:

- `python -m py_compile tests/benchmarks/run_writer_core_benchmarks.py` passed.
- `cargo check -p journal-core -p writer_core_bench` passed.
- `cargo test -p journal-core live_publication_modes_preserve_closed_file_bytes -- --nocapture`
  passed.
- `cargo test -p journal-core -p writer_core_bench` passed outside the sandbox:
  `58` journal-core tests, `0` writer-core-bench tests, and `1` doc-test
  passed; `3` doc-tests were ignored.
- `cargo test -p journal-core file::mmap::tests -- --nocapture` passed `5/5`
  mmap tests after the whole-file/post-change fix.
- `timeout 1800 python3 tests/interoperability/run_live_matrix.py --writers rust --entries 30 --writer-delay-ms 20 --poll-readers 2 --libsystemd-readers 1 --keep-files`
  passed `9/9` feature variants. Result:
  `.local/interoperability/live-feature-matrix-results-20260528-073251.json`.
- A sandboxed `cargo test -p journal-core -p writer_core_bench` attempt failed
  only in stock `journalctl --verify` tests with `Failed to create data file:
  Read-only file system`; rerunning outside the sandbox passed.
- `cargo fmt --manifest-path rust/Cargo.toml --all` passed.
- `CARGO_HOME=.local/cargo-home CARGO_TARGET_DIR=.local/cargo-target cargo check -p journal-core -p journal-log-writer -p writer_core_bench --manifest-path rust/Cargo.toml`
  passed.
- `CARGO_HOME=.local/cargo-home CARGO_TARGET_DIR=.local/cargo-target cargo test -p journal-core -p journal-log-writer -p writer_core_bench --manifest-path rust/Cargo.toml`
  passed outside the sandbox: `58` journal-core tests, `2`
  journal-log-writer unit tests, `46` log-writer integration tests, `0`
  writer-core-bench tests, and `2` doc-tests passed; `3` doc-tests were
  ignored.
- `gofmt -w go/journal/writer.go go/journal/log.go go/journal/writer_test.go go/internal/testcmd/writer_core_bench/main.go`
  passed.
- `GOCACHE=$PWD/.local/go-cache GOMODCACHE=$PWD/.local/go-mod-cache GOPATH=$PWD/.local/go-path go test ./...`
  passed outside the sandbox. The first sandboxed run failed only in stock
  `journalctl --verify` tests with `Failed to create data file: Read-only file
  system`.
- `node --check node/src/lib/writer.js && node --check node/src/lib/directory-writer.js && node --check node/internal/testcmd/writer-core-bench.js && node --check node/test/all.js`
  passed.
- `npm test` passed outside the sandbox. The first sandboxed run failed only in
  stock `journalctl --verify` with `Failed to create data file: Read-only file
  system`.
- `PYTHONPATH=python python3 -m py_compile python/journal/writer.py python/journal/directory_writer.py python/cmd/writer_core_bench.py python/test_all.py tests/benchmarks/run_writer_core_benchmarks.py`
  passed.
- `PIP_CACHE_DIR=$PWD/.local/pip-cache python3 -m pip install --target .local/python-deps lz4==4.4.5`
  installed the documented Python LZ4 dependency under `.local/`.
- `PYTHONPATH=python:.local/python-deps python3 python/test_all.py` passed
  outside the sandbox. The first sandboxed run failed only in stock
  `journalctl --verify`; the first unsandboxed run without `.local/python-deps`
  failed because `lz4` was missing.
- `python3 tests/benchmarks/run_writer_core_benchmarks.py --languages rust go node python --rows 10 --repetitions 1 --warmups 0 --format compact --final-state online --max-size-bytes 134217728 --rust-api-mode structured-field --rust-trusted-unique-payloads --live-publish-every-entries 64 --skip-verify --output-dir .local/benchmarks/sow36-validation`
  passed and produced
  `.local/benchmarks/sow36-validation/compact-none-fss-off-rust-structured-field-trusted-unique-live-every-64-mmap-windowed-20260528T053041286339Z/report.json`.
- Direct pinned Rust/Go default benchmark, alternating on one CPU with
  `taskset -c 3`, passed and recorded Rust median `43922.493 rows/sec` and Go
  median `43559.652 rows/sec`:
  `.local/benchmarks/sow36-rust-go-pinned-nosandbox-20260528T060632Z/`.
- Direct pinned Rust/Go live-mode benchmark, alternating on one CPU with
  `taskset -c 3`, passed and recorded:
  - `live_publish_every_entries=64`: Rust median `47092.770 rows/sec`, Go
    median `46504.753 rows/sec`.
  - `live_publish_every_entries=0`: Rust median `50159.448 rows/sec`, Go
    median `48853.103 rows/sec`.
  Result:
  `.local/benchmarks/sow36-rust-go-pinned-live-modes-20260528T060808Z/`.

Real-use evidence:

- Stock `journalctl --verify --file` passed for all benchmark measurement
  journal files in every Rust publication/mmap mode.
- Stock libsystemd live readers observed all `30` entries in every Rust live
  matrix feature variant for the default immediate mode.
- Go, Node.js, and Python package tests that invoke stock `journalctl --verify`
  passed outside the sandbox after the environment issue and missing Python
  dependency were removed.

Reviewer findings:

- Reviewer pass started on 2026-05-28 with `kimi-k2.6`, `qwen3.6-plus`,
  `glm-5.1`, and `minimax-m2.7-coder`.
- `qwen3.6-plus`: `PRODUCTION GRADE`. Non-blocking observations:
  experimental Rust types are public despite `#[doc(hidden)]`; summary
  aggregation is less useful for mixed-mode benchmark runs.
- `glm-5.1`: `PRODUCTION GRADE`. Non-blocking observations:
  `#[doc(hidden)]` types remain publicly reachable; no explicit negative test
  proves disabled mode does not wake stock follow readers during active writes.
  Disposition: the API contract is that non-default modes disable or batch the
  SDK's explicit publication operation and must not be claimed stock-follow
  compatible without exact-mode matrix evidence. It is not a guarantee that the
  kernel or a polling reader will never observe writes, so a negative wakeup
  test is not required for this SOW.
- `minimax-m2.7-coder`: `BLOCKING`. Finding claimed a stale whole-file mmap
  window could be returned after `post_change()` grows a writer-owned file.
  Disposition: the trigger scenario had a false premise because
  `create_window()` extends writer-owned files before `MmapMut::map_mut()` and
  `Window::contains_range()` cannot claim bytes beyond the mapped window size.
  Added `whole_file_writer_owned_remaps_after_post_change_growth` to cover the
  exact boundary-growth case; the targeted test passed.
- `kimi-k2.6`: `BLOCKING`. Finding identified a real SIGBUS risk when a mapped
  writer window is larger than the journal arena growth increment:
  `post_change()` could truncate the file below an oversized existing mapping.
  Disposition: fixed by dropping active windows before `post_change()` performs
  a shrinking `set_len()`. Also moved remap counters so failed remap attempts
  do not inflate successful remap stats. Added
  `post_change_drops_mappings_before_truncating_oversized_windows`; mmap tests
  and full Rust tests passed after the fix.
- Second reviewer pass after the SIGBUS/remap fixes:
  - `kimi-k2.6`: `PRODUCTION GRADE`. Verified SIGBUS fix, remap counter fix,
    default stock compatibility, closed-file byte compatibility, whole-file
    mmap safety, and conservative benchmark interpretation. Non-blocking
    public `#[doc(hidden)]` experimental-type observation remains recorded for
    any future public API graduation.
  - `qwen3.6-plus`: `PRODUCTION GRADE`. Verified the same fix set and noted
    the same non-blocking public experimental-type/API-surface observation,
    summary aggregation observation, and missing negative live-follow test
    before public API graduation.
  - `glm-5.1`: `PRODUCTION GRADE`. Verified `post_change()` drops mappings
    before shrinking, remap count increments only after successful remap,
    whole-file growth remaps correctly, and default behavior remains
    stock-compatible.
  - `minimax-m2.7-coder`: `PRODUCTION GRADE`. Verified previous
    SIGBUS/stale-mmap findings are fixed, mmap tests pass, sandboxed
    `journalctl` failures are environment-only, and no blocking correctness,
    safety, mmap lifetime, default behavior, benchmark validity, or
    maintenance concerns remain.

Same-failure scan:

- The same sandbox-only `journalctl --verify` temporary-file failure was seen
  in benchmark verification plus Rust, Go, Node.js, and Python tests. Rerunning
  those commands outside the sandbox passed, so the failure is recorded as
  environment-related rather than a journal compatibility failure.
- The Python test dependency gap was specific to the local test environment:
  `lz4==4.4.5` is documented in `python/README.md` and was installed under
  `.local/python-deps` for validation.

Sensitive data gate:

- This SOW contains only repository paths, synthetic benchmark context, and
  aggregate performance discussion. It does not contain raw secrets, SNMP
  communities, customer identifiers, personal data, private endpoints, or
  production log data.

Artifact maintenance gate:

- `AGENTS.md`: no update needed; current workflow and compatibility guardrails
  still apply.
- Runtime project skills: `.agents/skills/project-journal-compatibility/SKILL.md`
  updated with the durable `live_publish_every_entries` compatibility rule.
- Specs: `.agents/sow/specs/product-scope.md` updated with the public cadence
  contract and default/narrowed compatibility distinction.
- End-user/operator docs: `rust/README.md`, `go/README.md`, `go/API.md`,
  `node/README.md`, `python/README.md`, and `tests/benchmarks/README.md`
  updated.
- End-user/operator skills: no output/reference skills are affected.
- SOW lifecycle: SOW-0036 completed and is moved to done with this change;
  SOW-0037 remains paused; `SOW-status.md` updated.

Specs update:

- `.agents/sow/specs/product-scope.md` now records the shared writer cadence
  API: default `1`, disabled `0`, and every-`N` publication. It also records
  that this is not a durability sync setting and that Node.js/Python direct
  writes may still produce kernel-visible write events.

Project skills update:

- `.agents/skills/project-journal-compatibility/SKILL.md` now requires future
  writer/reviewer work to label non-default publication modes and not claim
  stock follow-reader compatibility unless the exact mode is tested.

End-user/operator docs update:

- Language READMEs and Go API docs now document the option names and semantics.
- `tests/benchmarks/README.md` now explains the shared
  `live_publish_every_entries` benchmark label.

End-user/operator skills update:

- Not affected; there are no output/reference skills in this repository.

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

- No deferred item remains inside this SOW.
- SOW-0037 tracks the broader reference-drift audit and API parity work after
  this SOW closes.
- SOW-0009 tracks broad performance benchmarking/profiling after remaining
  feature work and reference-drift work settle.

## Outcome

Completed. The SDKs now expose a shared live publication cadence option across
Rust, Go, Node.js, and Python. The stock-compatible default remains publication
after every entry. Latency-tolerant consumers may select disabled publication
or every-N publication, but those modes are explicitly narrower contracts and
must not be described as stock `journalctl --follow` compatible without
mode-specific live matrix evidence.

The Rust whole-file mmap experiment was rejected for this workload because it
did not improve throughput. The Rust recent-DATA-cache size experiment was also
rejected because the controlled A/B did not show a benefit. The apparent initial
Go speed lead was not reproduced under pinned alternating Rust/Go measurements,
so no unproven Go-derived Rust optimization was kept.

## Lessons Extracted

- Benchmark protocol must be recorded with enough detail to avoid comparing a
  Go-only unpinned harness run against a pinned alternating direct-binary run as
  if they were the same test.
- Live-reader publication is a compatibility surface, not a durability surface.
  API names, docs, and benchmark labels must keep that distinction explicit.
- Performance options should stay benchmark/internal until they show a material
  benefit under the workload that justifies their semantic and maintenance cost.

## Followup

None for this SOW. Related remaining work is already tracked by SOW-0037 and
SOW-0009.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and
later testing or use found broken behavior. Use a dated `## Regression -
YYYY-MM-DD` heading at the end of the file. Never prepend regression content
above the original SOW narrative.
