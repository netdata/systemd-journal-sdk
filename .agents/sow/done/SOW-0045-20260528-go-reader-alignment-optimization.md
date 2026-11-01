# SOW-0045 - Go Reader Alignment Optimization

## Status

Status: completed

Sub-state: regression repair completed 2026-11-01; decisions 1-9 implemented
and validated in Go and Rust, including v262 CLI re-certification.

Original completion: 2026-05-29. Go reader now has Rust-aligned payload
APIs where practical, mmap-backed Unix access, live/snapshot bounds,
byte-preserving RAW field access, facade DATA fast paths, Go reader benchmark
coverage, and recorded benchmark evidence.

Regression reopened 2026-10-15: healthy Go refresh can skip the first newly
appended entry when the reader was empty or at forward EOF. This SOW owns the
original refresh implementation and live-reader claim. It is queued/paused
under the one-active-SOW rule; prerequisite SOW-0108 and SDK SOW-0159 are
completed, with journalctl SOW-0121 next after the signed SDK commit.
Do not insert this separate positioning repair into the approved
SOW-0108 -> SOW-0159 -> SOW-0121 execution chain without a dependency finding
or a user scheduling decision.

2026-10-31: SOW-0108, SOW-0159 and SOW-0121 are all completed, so that
ordering no longer blocks this SOW. It waits only for activation in the
user-approved backlog order recorded in `.agents/sow/SOW-status.md`. Original completion evidence remains below.

## Requirements

### Purpose

Align the Go reader with the optimized Rust reader reference and make it fit for
Netdata reader use cases where Go is relevant.

### User Request

After Rust reader parity and optimization, the user wants Go fixed and
optimized next. The user also noted that the Go writer uses mmap while the Go
reader does not, and that this likely affects performance.

### Assistant Understanding

Facts:

- Go reader currently uses `ReadAt()`-style reads rather than mmap-backed object
  access.
- Go reader must support ordered directory reading and shared reader API
  behavior.
- Go reader performance must be measured separately for single-file and
  directory readers.

Inferences:

- Go reader should be compared against optimized Rust, not the current
  pre-optimization Rust reader.

Unknowns:

- Whether Go mmap reader access should be default, optional, or avoided after
  measurement.

### Acceptance Criteria

- Go reader behavior matches the Rust reader contract from SOW-0043.
- Go reader performance is measured for single-file and ordered directory
  reading.
- Go reader mmap versus `ReadAt()` behavior is measured and decided with
  evidence.
- Go reader passes shared reader conformance and mixed-directory tests.
- Go reader performance gaps versus Rust are profiled and either fixed or
  explicitly recorded.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `go/journal/reader.go`
- `.agents/sow/specs/product-scope.md`

Current state:

- Go reader is functionally capable but likely not optimized for hot-path
  reader use.

Risks:

- Mmap can improve read hot paths but adds resize/SIGBUS/platform complexity.
- Optimizing Go before Rust reader closure could copy the wrong behavior.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Go reader should follow the optimized Rust reader reference from SOW-0052 and
  the Python/Node.js parity lessons from SOW-0053/SOW-0054.
- The current Go reader is functionally capable but still uses `ReadAt()` and
  allocates/copies heavily in the entry hot path. It also lacks the current Rust
  reader payload API layer used for high-throughput scans and facade DATA
  enumeration.
- The current reader-core benchmark harness does not include Go reader cases, so
  Go has no trustworthy apples-to-apples reader performance number yet.

Evidence reviewed:

- Current Go reader implementation and product scope spec.
- `.agents/sow/done/SOW-0052-20260529-rust-reader-last-mile-optimization.md`
  for optimized Rust reader contract and benchmark baseline.
- `.agents/sow/done/SOW-0053-20260529-python-reader-writer-rust-port.md` and
  `.agents/sow/done/SOW-0054-20260529-node-reader-writer-rust-port.md` for
  cross-language parity API shape.
- `tests/benchmarks/run_reader_core_benchmarks.py` currently lists Rust, Python,
  Node.js, and systemd reader cases but no Go reader cases.
- `go/journal/reader.go` currently materializes full entries with repeated
  `ReadAt()` calls and per-field copies in `readEntryAt()`.

Affected contracts and surfaces:

- Go reader API, directory reader, libsystemd-compatible facade, journalctl
  rewrite, and Netdata integration readiness.

Existing patterns to reuse:

- Optimized Rust reader behavior from SOW-0052.
- Go writer mmap support where applicable.
- Shared reader fixtures.
- Python and Node.js current-entry payload APIs and facade fast paths where Go
  cannot expose Rust borrowed slice lifetimes exactly.

Risk and blast radius:

- High for Go reader consumers once integrated.

Sensitive data handling plan:

- Use generated or public fixtures only. Do not record real customer logs,
  SNMP communities, credentials, bearer tokens, personal data, private
  endpoints, or production incident details.

Implementation plan:

1. Compare Go reader against Rust reader contract.
2. Add missing API/behavior parity.
3. Benchmark `ReadAt()` and mmap strategies.
4. Optimize based on profiles.
5. Validate conformance and performance.

Validation plan:

- Go tests.
- Shared reader conformance and mixed-directory tests.
- Benchmark/profiler artifacts.
- Read-only reviewer passes.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: update only if Go reader workflow changes.
- Specs: update Go reader feature/performance status.
- End-user/operator docs: update Go reader docs.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: activate after Rust reader optimization.
- SOW-status.md: update when activated and closed.

Open-source reference evidence:

- systemd/libsystemd evidence comes through SOW-0043 unless new evidence is
  required.

Open decisions:

- Go mmap default/option decision requires measurement. Default should remain
  conservative unless benchmark and validation evidence justify changing it.

## Implications And Decisions

- 2026-05-28: user agreed Go reader follows Rust reader parity and
  optimization.
- 2026-05-29: Go reader default is mmap-backed live reading on Unix. `ReadAt`
  remains an explicit diagnostic/constrained-environment option because the
  benchmark measured `ReadAt` around 17k rows/s versus mmap around 1.07M
  rows/s for single-file payload scans.
- 2026-05-29: Go current-entry payload enumeration intentionally returns
  borrowed reader-owned slices for the zero-copy hot path, matching Rust's
  borrowed `&[u8]` model. `CollectEntryPayloads()`, `GetEntryPayload()`,
  `GetRaw()`, and `GetRawValues()` provide owned copies when callers need
  ownership.
- 2026-05-29: Go `SdJournalSeekCursor()` keeps the systemd/libsystemd
  no-existence-proof behavior already accepted for Python and Node.js. A
  reviewer found that current Rust still returns an error for a syntactically
  valid but nonexistent cursor; this is tracked as SOW-0055 so Rust can be
  realigned to systemd instead of regressing Go.

## Plan

1. Align Go reader API and internals with the finalized Rust/Python/Node.js
   current-entry payload model.
2. Add mmap-backed reader access and live/snapshot bounds.
3. Add Go reader benchmark coverage and compare mmap versus `ReadAt`.
4. Validate shared conformance, mixed-directory, live, and benchmark paths.

## Delegation Plan

Implementer:

- Local implementation by the project manager unless the user changes routing.

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

- Record mmap findings, performance gaps, reviewer findings, and audit failures
  in this SOW.

## Execution Log

### 2026-05-28

- Created from the agreed SOW restructuring.

### 2026-05-29

- Activated after the user requested Rust parity in Go reader code plus
  benchmark evidence.
- Implemented `ReaderOptions`, `ReaderAccessMode`, `ReaderBounds`,
  `OpenFileWithOptions`, `OpenDirectoryWithOptions`, and
  `OpenFilesWithOptions`.
- Added Unix read-only mmap access for the Go reader and a non-Unix whole-file
  buffer fallback behind the same API.
- Added active-file live refresh at tail/end and snapshot bounds for polling
  sessions that should not see appends during a scan.
- Added byte-preserving RAW field-name representation through `RawField`,
  `Entry.RawFields`, `Entry.RawFieldValues`, `Entry.Raw()`,
  `Entry.RawValues()`, `Reader.GetRaw()`, and `Reader.GetRawValues()`.
- Added current-entry payload APIs: `VisitEntryPayloads()`,
  `CollectEntryPayloads()`, `GetEntryPayload()`, `EntryDataRestart()`, and
  `EnumerateEntryPayload()`.
- Updated the libsystemd-compatible facade so `SdJournalGetData()` and
  restart/enumerate DATA paths use current-entry payload access instead of full
  entry materialization.
- Added ordered directory fast path for strict non-overlapping seqnum and
  realtime file ranges, with generic merge retained for overlapping files,
  filters, realtime seeks, and direction-change cases.
- Added `go/internal/testcmd/reader_core_bench` and integrated Go reader cases
  into `tests/benchmarks/run_reader_core_benchmarks.py`.
- Added regression tests for RAW byte field payload APIs, live versus snapshot
  bounds, and non-overlapping directory fast-path forward/backward ordering.
- Updated Go API docs, Go README, benchmark README, and product scope spec.
- First read-only review round found one useful documentation gap around
  borrowed mmap-backed slices. Added code comments and public docs while
  preserving the zero-copy hot path.
- Second read-only review round raised a Go/Rust `SdJournalSeekCursor()`
  divergence. Verified upstream systemd behavior in
  `systemd/systemd @ cf3156842209`,
  `src/libsystemd/sd-journal/sd-journal.c:1263-1363`: systemd stores the
  requested location and returns success without proving the cursor exists.
  Go behavior is correct for the libsystemd-compatible facade; Rust divergence
  is tracked in SOW-0055.

## Validation

Acceptance criteria evidence:

- Go reader behavior matches the accepted cross-language reader contract for
  mmap/live/snapshot access, current-entry payload scans, byte-preserving RAW
  field names, directory ordering, and libsystemd-compatible facade DATA
  enumeration.
- Mmap versus `ReadAt` was measured in
  `.local/benchmarks/reader-core-go-parity-final/20260529T112718Z`.
- Final benchmark median rows/s for compact 100k-row fixture:
  - Go single-file `sdk-payloads` live/mmap: 1,067,776.
  - Go single-file `facade-data` live/mmap: 1,091,091.
  - Go single-file `sdk-entry` live/mmap: 69,450.
  - Go single-file `sdk-payloads` live/`ReadAt`: 17,425.
  - Go open-files `sdk-payloads` live/mmap: 697,265.
  - Go open-files `facade-data` live/mmap: 606,245.
  - Rust single-file `sdk-payloads` live/windowed: 2,471,020.
  - Rust open-files `sdk-payloads` live/windowed: 1,578,940.
  - systemd single-file DATA enumeration: 564,858.
  - systemd open-files DATA enumeration: 531,905.
- Go mmap payload/facade paths are faster than stock systemd for the measured
  single-file and open-files DATA enumeration baselines. Rust remains faster,
  especially on the payload hot path; this is recorded as the current residual
  performance gap.

Tests or equivalent validation:

- `go test ./journal ./internal/testcmd/reader_core_bench` passed.
- `go test ./...` passed.
- `python -m py_compile tests/benchmarks/run_reader_core_benchmarks.py` passed.
- `PYTHONPATH=.local/python-deps:python python tests/interoperability/run_mixed_directory_matrix.py --readers go rust stock` passed 42/42.
- `PYTHONPATH=.local/python-deps:python python tests/interoperability/run_matrix.py --entries 200 --writers go rust --readers go rust stock` passed 32/32.
- `PYTHONPATH=.local/python-deps:python python tests/interoperability/run_live_matrix.py --entries 100 --features regular compact --writers go rust --readers go rust stock --poll-readers 1 --libsystemd-readers 1 --writer-delay-ms 1` passed 4/4.
- `python tests/benchmarks/run_reader_core_benchmarks.py --rows 100000 --directory-rows 100000 --repetitions 3 --warmups 1 --languages rust,go,systemd --out .local/benchmarks/reader-core-go-parity-final` passed checksum validation.
- `git diff --check` passed.

Real-use evidence:

- Go reader benchmark command builds and is exercised by the shared reader-core
  harness.
- Mixed-directory and live matrices exercised Go readers against repository
  writer output and stock reader paths.

Reviewer findings:

- Initial whole-SOW read-only review:
  - Minimax reported several findings; most were false positives after code
    verification. The useful outcome was extra scrutiny of mmap fallback,
    directory fast path, and `SdJournalSeekCursor()`.
  - Qwen reported the borrowed mmap-backed slice lifetime gap for
    `EnumerateEntryPayload()` / facade data enumeration. Disposition: accepted
    as a documentation/API contract gap, fixed with comments and docs while
    preserving zero-copy behavior.
  - GLM independently reported the same borrowed-slice documentation gap and
    verified the core reader paths. Disposition: fixed.
  - Kimi stalled without a final review and its verified reviewer PIDs were
    stopped.
- Second whole-SOW read-only review after fixes:
  - GLM reported one high-severity `SdJournalSeekCursor()` concern because Go
    now differs from current Rust. Disposition: rejected as a Go blocker after
    direct upstream systemd verification; created SOW-0055 to correct Rust
    parity with systemd.
  - GLM reported a medium residual risk that the non-overlapping directory fast
    path is computed at open time. Disposition: accepted as a residual
    low-practical-risk invariant for standard systemd/SDK rotation, where only
    the last active file can grow; generic merge remains in use when the
    initial ranges overlap, filters are active, realtime seek is pending, or
    direction changes from a current entry.
  - Qwen repeatedly ran Go tests from the repository root where no Go module
    exists and exited without a usable final review. Local `go test ./...` from
    `go/` passed.
  - Minimax became silent for several minutes without final output and its
    verified reviewer PIDs were stopped.

Same-failure scan:

- Searched and verified current-entry payload ownership paths:
  `VisitEntryPayloads()` and `EnumerateEntryPayload()` are documented borrowed
  paths; `CollectEntryPayloads()`, `GetEntryPayload()`, `GetRaw()`,
  `GetRawValues()`, `GetEntry()`, export, and JSON paths clone/own data.
- Searched cursor behavior across Go, Rust, Python, Node.js, and upstream
  systemd. Go/Python/Node match systemd no-existence-proof behavior; current
  Rust exact-search behavior is tracked by SOW-0055.
- Added explicit fast-path regression coverage for non-overlapping directory
  ordering in both directions.

Sensitive data gate:

- Durable artifacts contain generated-fixture paths, benchmark rates, and
  public upstream source references only. No raw secrets, credentials, SNMP
  communities, bearer tokens, customer names, personal data, non-private
  customer-identifying IPs, private endpoints, or proprietary incident details
  were recorded.

Artifact maintenance gate:

- AGENTS.md: no workflow or project-wide guardrail change.
- Runtime project skills: no workflow change requiring skill update.
- Specs: updated `.agents/sow/specs/product-scope.md`.
- End-user/operator docs: updated `go/README.md`, `go/API.md`, and
  `tests/benchmarks/README.md`.
- End-user/operator skills: no output/reference skill affected.
- SOW lifecycle: activated SOW-0045; created follow-up SOW-0055 for Rust cursor
  parity discovered during review.
- SOW-status.md: updated for SOW-0045 activation and SOW-0055 pending tracking.

Specs update:

- Updated `.agents/sow/specs/product-scope.md` with Go mmap default and
  current-entry payload hot-path contract.

Project skills update:

- No project skill update needed; no new reusable workflow rule was introduced.

End-user/operator docs update:

- Updated `go/README.md`, `go/API.md`, and `tests/benchmarks/README.md`.

End-user/operator skills update:

- No output/reference skill affected.

Lessons:

- Go `ReadAt` reader performance is not production-viable for Netdata-style
  hot paths; mmap must be the Unix default.
- Go can match Rust's zero-copy payload model only by documenting borrowed slice
  lifetime explicitly.
- Reviewers may flag current Rust as the reference even when later SOWs already
  verified different upstream systemd behavior; resolve these with source
  evidence, then track true Rust drift separately.

Follow-up mapping:

- SOW-0055 tracks the discovered Rust `seek_cursor()` divergence from upstream
  systemd no-existence-proof behavior.
- The remaining Go-vs-Rust throughput gap is recorded as a measured residual:
  Go beats systemd in the measured payload/facade paths but remains below Rust.
  No separate follow-up is opened until the user prioritizes deeper Go reader
  micro-optimization beyond the current systemd-beating baseline.

## Outcome

Completed. Go reader behavior and benchmark coverage are now aligned with the
accepted cross-language reader contract for this phase. The measured Go mmap
payload/facade paths beat stock systemd on the compact 100k-row reader-core
baseline, while Rust remains faster and is recorded as the current upper-bound
implementation.

## Lessons Extracted

- The fastest Go reader path must avoid full entry materialization and use mmap
  plus current-entry payload iteration.
- Go's type system cannot express Rust borrowed-slice lifetimes, so public docs
  and comments must clearly distinguish borrowed zero-copy APIs from owned-copy
  APIs.
- Rust remains the performance reference, but direct upstream systemd evidence
  overrides Rust when facade semantics differ from libsystemd.

## Followup

- SOW-0055 tracks Rust `seek_cursor()` alignment with upstream systemd
  no-existence-proof behavior.

## Regression Log

- 2026-10-15: healthy refresh skipped the first append, plus related
  directory-seek and time-search gaps. Repaired and validated 2026-11-01; see
  `## Regression - 2026-10-15`.

## Regression - 2026-10-15

### What Broke And Evidence

Healthy Go iteration can skip the first append after an explicit Refresh or a
unique-field query that refreshes the entry list. Two states reproduce it:
open an empty valid journal, append one row, refresh and Step; or consume a
one-row journal through EOF, append one row, refresh and Step. Both Step calls
return false/nil despite the newly loaded row. QueryUnique triggers the same
transition. A private diagnostic against unchanged HEAD `dafcdac4b971` reproduces
all four combinations; retain `sow-0045-refresh-position-probe.log` and its
synthetic test source under ignored task scratch.

At HEAD, `go/journal/reader.go:606-609` leaves the empty constructor index at
zero, successful refresh restores the old numeric index at `:561-562`, and
Next increments it before selecting a row. At populated forward EOF the old
index equals the old row count; refresh then makes that index refer to the
first append, which the next increment skips. Automatic Step-driven tail
refresh has a separate adjustment and does not establish that explicit refresh
works. `go/journal/reader_unique.go` and directory unique-cache refresh call
the same refresh path. Commit `6a48ccc5` introduced that preservation behavior
in this SOW; the original execution/validation above claimed live refresh and
live/snapshot parity.

### Why Earlier Validation Missed It

Existing live tests begin with a populated reader and use automatic tail
refresh. Explicit-refresh retained-row tests refresh while positioned on a row
and do not subsequently step from EOF. The new SOW-0108 header test initially
assumed empty-open positioning; its intended header/array-growth coverage now
explicitly SeekHead before appending. That adjustment does not repair or claim
acceptance of this separate positioning defect.

### Repair Plan And Validation

Keep this queued until scheduled. Before code, specify position preservation
for empty/head/unset, a current row, forward EOF, explicit tail, reverse
movement, repeated/no-op refresh and unique-triggered refresh. A blanket
old-length-minus-one adjustment is not an established complete repair: it can
change reverse movement or expose a current row before a successful Step.
Cover public Reader, facade and directory callers, regular/compact files,
mmap/ReadAt, tiny/default windows, retained payloads, failed-refresh rollback
and snapshot exclusion. Require appended rows exactly once and preserve native
index paths and measured performance.

Rust is not a confirmed passing oracle for this transition. Source analysis
suggests a possible healthy-tail cached-population issue; this is an unexecuted
working theory requiring a separate paired runtime check before any Rust
finding or scope decision. SOW-0159 already handles its distinct empty-file and
recovery contracts; do not silently broaden that SOW to this healthy-position
state machine.

### Artifact Impact And Follow-Up Mapping

The originating SOW is reopened here rather than replaced by a new SOW.
Canonical/root status indexes record its paused state, and SOW-0159 and
SOW-0108 cross-reference this finding. No positioning runtime code is changed.
Synthetic fixtures contain no real journal data. During repair, update the
product live-reader contract, caller guidance and compatibility skill if the
resolved behavior requires it; preserve existing compiler/purity constraints.
Validation, review disposition and a complete-and-clean audit remain required
before this regression can close. This is tracked work outside the active
header/lifetime repair; no completion is claimed.

### Related Queued Directory Seek-State Finding - 2026-10-16

Read-only SOW-0159 impact analysis finds an independent pre-existing getter-state
gap: `go/journal/directory_reader.go:739` clears realtime/candidate state during
SeekRealtimeUsec but retains the selected file index. After a successful Step,
SeekRealtimeUsec followed by GetEntry/GetCursor can therefore expose the old
row before movement. Rust's `rust/src/journal/src/directory.rs:201-208` clears
its index and its getters report NoEntry until movement succeeds. The Go
omission exists in commit `fcca32fe56f4`; SOW-0027 claimed broad facade and seek
support but did not specify getter state between seek and movement.

Track this alongside the queued healthy-position analysis here; it is not a
consequence of native filtering and adds no SDK-first prerequisite. Before any
runtime change, reproduce both public APIs and facades, determine the affected
seek/refresh state combinations and obtain any unresolved behavior decision.

### Additional Unfiltered Time-Search Evidence - 2026-10-17

SOW-0159's directory error-position controls find another inherited unfiltered
Go seek gap. `reader.go:firstRealtimeIndexAtOrAfter` and
`lastRealtimeIndexAtOrBefore` use a `sort.Search` predicate that treats an ENTRY
read error as a comparison result and then returns a nil error. A backward
merged-directory direction change can consequently skip the damaged candidate
without reporting its ENTRY-read failure. The same source pattern exists in
the frozen SOW-0159 baseline. This is separate from retaining the previous row
after an error that is actually returned.

Track propagation and retry of errors from unfiltered time probes here with
the existing seek/refresh scope. The approved SOW-0159 binary-search change is
for explicit filtered realtime positioning, and its ordinary unfiltered time
policy remains unchanged. Its Go directory position regression matrix excludes
the unfiltered backward merged case for that specific reason; filtered cases
and sequential unfiltered file transitions remain covered. Before implementing
this item, add a direct file and directory SDK/facade reproducer, compare Rust
and the frozen baseline, resolve any remaining behavior choice, and measure
the existing/new binary-search paths. This SOW remains paused while SOW-0159
is the sole implementation SOW.
No new position contract or implementation is approved by this tracking note.
SOW-0159 retains its distinct reverse-filter and selected-entry error decisions.

### Position Contract Analysis - 2026-10-31

Activated as backlog item 1. Evidence gathered with throwaway probes under
`.local/sow0045-probe/` (ignored): a Go public-API probe, a Rust `FileReader`
probe, and a stock libsystemd 255 probe (`sd_journal_open_files`). Each opens
the reader, enters a state, the Go writer appends one row and syncs, then
the reader calls next or previous once. Rows are identified by realtime usec.

systemd reference: `systemd/systemd @ c3cd6e5bdb07` (v260.1)
`src/libsystemd/sd-journal/sd-journal.c:943-944` (HEAD/TAIL find-location),
`:1013-1027` (EOF continuation from the last returned entry).

| State, then append one row | stock libsystemd next / previous | Go auto refresh | Go `Refresh` / `QueryUniqueState` | Rust (auto and `query_unique_state`) |
|---|---|---|---|---|
| empty open | row1 / none | row1 / none | none / none | row1 / none |
| seek head | row1 / none | row1 / none | row1 / none | row1 / none |
| on last row | row2 / none | row2 / none | row2 / none | none / none |
| forward EOF | row2 / none | row2 / row1 | none / row1 | none / none |
| explicit tail | none / row2 | row2 / row1 | none / row1 | none / row2 |
| failed previous at head | row2 / none | row1 / none | row1 / none | none / none |

Findings:

1. Go explicit refresh (reported regression): empty open and forward EOF skip
   the first appended row. Root cause: position is an array index; the empty
   constructor leaves it at 0 instead of -1 (`go/journal/reader.go:651-662`),
   and refresh restores the old numeric index (`:588`), whose "after tail"
   value refers to the first appended row once the array grows; `Next`
   increments past it (`:827-846`).
2. Go explicit tail: Go treats SeekTail as "after current last row", so with
   auto refresh next returns appended rows and previous returns the old last
   row; systemd keeps a TAIL location (next always none, previous the newest).
3. Go boundary reversal: Go models EOF as a position past the end, so previous
   after forward EOF re-returns the last row, and next after a failed previous
   at head re-returns the first row. systemd keeps the last returned entry as
   the location and moves beyond it.
4. Rust live continuation (the earlier unexecuted working theory is now
   confirmed): once positioned on or after an entry, `FileReader::next` never
   returns appended rows, although `seek_head` plus iteration sees the full
   population (probe "recount": rows2 in every state). Head/tail-anchored
   states work.
5. Directory seek getters and unfiltered time-probe error suppression, recorded
   above, remain in scope.

Netdata exposure: none of these paths are used by Netdata today (it uses
`IndexedSnapshot`, `VisitRange`/`VisitMatch`, `VerifyIndex` and the logs
function); finding 5's time-probe errors are reachable through the Go logs
function on damaged files only.

Open user decisions are recorded in Implications And Decisions below once made.

### User Decisions - 2026-10-31

1. A - Go and Rust next/previous behavior matches stock libsystemd for every
   state in the table above (head, tail, entry location, EOF continuation and
   boundary reversal), for idiomatic readers and facades.
2. A - The Rust live-continuation bug is fixed in this SOW, so one position
   contract ships in both languages.
3. A - Go directory readers report no current entry between a seek and the
   next successful movement, matching Rust and libsystemd.
4. A - Errors reading ENTRY objects during unfiltered realtime search are
   returned instead of being treated as comparison results.

### Filtered Position Evidence - 2026-10-31

Same probes with one match (`K=v`, present on every row). Stock libsystemd
returns exactly the unfiltered table. Go filtered readers return none for next
in the on-last-row, forward-EOF and failed-previous states under every refresh
path, and an empty-open reader with auto refresh returns none for next but
row1 for previous. Rust filtered readers fail the same states as unfiltered.
Decision 1 covers filtered readers: the position contract does not depend on
whether a match is installed.


### Go Implementation Progress And Population Conflict - 2026-10-31

- Go reader, filter, explorer and directory position changes now match stock
  libsystemd in every probe cell (unfiltered and filtered; auto refresh,
  `Refresh` and `QueryUnique`).
- Getters after a failed move keep returning the last returned entry, as
  libsystemd does: `real_journal_next` returns 0 without clearing
  `j->current_file` (`systemd/systemd @ c3cd6e5bdb07`
  `src/libsystemd/sd-journal/sd-journal.c:1163-1164`). Only an explicit tail
  has no current entry. This kept the existing EOF-position tests passing.
- Conflict found: three Go tests assert SOW-0159's contract that installed
  filters keep their captured DATA posting population on a healthy append
  (`TestNativeFilterIORetryPreservesCapturedPopulation`,
  `TestNativeFilterNewGroupsObserveHealthyAppend`,
  `TestNativeFilterGroupVisibilityKeepsRawGrowth`; contract text in
  `done/SOW-0159-*` "healthy resolved posting populations remain fixed until a
  new builder is installed"). Stock libsystemd with an installed match returns
  appended matching rows. Decision 1 cannot be applied to filtered live follow
  without reversing that SOW-0159 contract, so implementation stopped for a
  user decision.

### User Decision - 2026-10-31 (population conflict)

5. A - Installed filters observe healthy appended rows, matching stock
   libsystemd. This supersedes SOW-0159's "healthy resolved posting populations
   remain fixed" contract for healthy growth; its tests, spec text and code
   comments are updated. Damaged-population (I/O retry, recovery) handling keeps
   its existing rules.

### Implementation - 2026-10-31

Go (`go/journal`):

- `reader.go`: a `readerEdge` location (`edgeTail`, `edgeAfter`,
  `edgeBefore`) replaces out-of-range indexes. An explicit tail has no current
  entry; next stays at the tail and previous refreshes and returns the newest
  entry. A failed move parks on the last returned entry, which stays current
  for getters (libsystemd `real_journal_next` returns 0 without clearing
  `j->current_file`, `sd-journal.c:1163-1164`). Failed realtime-seek moves keep
  the seek location. Realtime binary searches return ENTRY read errors
  (decision 4). Empty entry arrays reset the index to the head.
- `reader_filter.go`, `reader_filter_step.go`, `reader_filter_recovery.go`:
  installed filters rebuild posting cursors when a live refresh changes the
  entry population; a forward step that finds nothing refreshes once and
  retries. Absent or empty DATA leaves keep their payload and are resolved
  again on refresh, as libsystemd looks DATA up by hash on every movement
  (`sd-journal.c:666-696`). Decision 5.
- `directory_reader.go`, `reader_cursor_direction.go`: a `directoryBoundary`
  head/tail location without an entry key; previous at the head and next at
  the tail return nothing. Sequential EOF keeps the last key as the location.
  `SeekRealtimeUsec` clears the current entry (decision 3).
- `explorer.go`: explorer positioning resets the edge.

Rust:

- `journal-core/src/file/cursor.rs`, `offset_array.rs`: a forward array-cursor
  step that ends re-resolves from the current offset when the entry count
  changed since the cursor was resolved (cold path).
- `journal-core/src/file/reader.rs`: `JournalReader` records the source
  matches and operations of the installed filter and rebuilds it when the
  entry count changed: before backward steps from tail/realtime locations and
  once after a forward step finds nothing (cold paths). The public
  `FilterExpr` type is unchanged; a raw `FilterExpr` driven directly remains a
  resolved snapshot (`recovered_filter_tail_retry_preserves_captured_populations`).
- `journal/src/directory.rs`: the same `DirectoryBoundary` state and sequential
  EOF behavior as Go. Rust already cleared the current entry on realtime seek
  and propagated realtime-search ENTRY errors (`cursor.rs` predicate uses
  `entry_ref(..)?`).

Tests:

- Go `reader_position_contract_test.go`: the libsystemd table for Reader,
  DirectoryReader and facade x regular/compact x mmap/ReadAt x unfiltered/
  filtered x refresh/no refresh (576 cells), absent match after append, getters
  after a failed move, merged directory boundaries/EOF, directory realtime seek
  (decision 3) and realtime search errors with retry (decision 4).
- Go `reader_filter_position_test.go`: re-enabled the merged unfiltered
  backward entry-load-error case that SOW-0159 excluded for this SOW.
- Go SOW-0159 tests updated to decision 5: `TestNativeFilterIORetryObservesHealthyAppend`
  (renamed), `TestNativeFilterNewGroupsObserveHealthyAppend`,
  `TestNativeFilterGroupVisibilityKeepsRawGrowth`.
- Rust `journal/src/tests/position_contract.rs`: the same table for
  `FileReader`, `DirectoryReader` and `SdJournal` x regular/compact x
  unfiltered/filtered, absent match, getters after failed move, merged
  directory, directory realtime seek.

Performance follow-up in implementation:

- First benchmark pass (`.local/sow0045-probe/bench-file`, 228 paired cases,
  5 repetitions) flagged Rust `core-next` whole-file (+12-18%) and Go
  `restart-first` live backward (~1.8x) and `facade-next` live ReadAt backward
  (+24%). Profiles showed the Go tail refresh calling `Stat` plus header
  `pread` per restart and, in ReadAt mode, invalidating every cached window.
- Fixes: Rust growth checks moved to `#[cold]` paths; Go tail and filtered EOF
  refreshes first read only the live header through the new accessor
  `liveHeader()` and run the full refresh only when the entry population moved
  (`Reader.liveEntriesChanged`, which also compares the loaded offset count
  because filter-group preparation can apply a newer header before entries
  load).
- Re-measure (`bench-focus`, `bench-focus2`, 15 repetitions): all Rust cases
  inconclusive or improved; Go `facade-next` ReadAt backward no longer
  regresses (manual runs 0.19-0.22 s baseline vs 0.20-0.21 s candidate). Go
  live `restart-first` backward stays ~1.4x (38.5 -> 52.0 ms for 10,000
  SeekTail+StepBack restarts, ~1.3 us each): one header `pread` per tail
  restart, the price of seeing live appends from an explicit tail. Snapshot
  bounds skip all new work; their 3-11% deltas were inconclusive in the
  previous run of the same cases and are treated as noise.

### Validation - 2026-10-31

- Go: `go vet ./journal/`; `CGO_ENABLED=0 go test -count=1 ./...` pass
  (adapter, writer_core_bench, journal, journalhost); `CGO_ENABLED=1 go test
  ./cmd/...` passes.
- Rust: `cargo test --workspace` passes; `cargo fmt --check` clean for the
  changed crates; clippy on `systemd-journal-sdk-core` and
  `systemd-journal-sdk` reports only warnings present before this SOW.
- Probes: Go and Rust match the stock libsystemd 255 oracle in every cell,
  unfiltered and filtered.
- Before/after: the new Go contract tests fail 228 of 576 matrix cells plus the
  absent-match, getter, merged-directory, directory-seek and realtime-error
  tests on the baseline export of `55b13ec`; the re-enabled SOW-0159 directory
  error case fails 8 cells on the baseline. All pass on the candidate.
- Interoperability (host systemd 255): `run_live_matrix.py --entries 10
  --readers go rust stock --features regular zstd compact sealed` 8/8 PASS;
  `run_directory_matrix.py --readers go rust stock` PASS;
  `run_mixed_directory_matrix.py` 42/42 PASS (all rerun on the final code);
  `run_reader_hardening.py --only chains` (204 observations) and `--only
  recovery` (272) PASS. `--only streams` stops in the stock-only verify step:
  host journalctl 255 reports zstd damage differently from the pinned oracle;
  no SDK code runs in that step.
- Not run: `run_journalctl_query_matrix.py` requires the pinned v262 oracle,
  which is no longer present on this host. CLI follow/query parity is
  therefore not re-certified by this SOW; CLI unit tests and the live matrix
  pass.

Self-review findings and dispositions:

1. Go getters after a failed unfiltered move now return the last entry
   instead of an error (spec updated). Matches libsystemd, Rust and Go's prior
   filtered behavior. Accepted under decision 1.
2. Directory readers (both languages) reported no current entry after a
   failed move, unlike libsystemd. Fixed under decision 7 (see below).
3. Raw Rust `FilterExpr` driven without `JournalReader` keeps its resolved
   population (public type unchanged). Recorded in the spec and recipe.
4. `cargo fmt --all` touched an unrelated `journal-registry` import; reverted
   by hand. The workspace has pre-existing rustfmt/clippy findings under the
   current toolchain; CI does not run them.

Same-failure search: Go and Rust file, directory and facade readers and the
explorer positioning paths were checked; the directory boundary gap existed
in both languages and is fixed in both.

Sensitive data: synthetic fixtures and identities only; no logs or host data
in durable artifacts.

Artifacts: spec `product-scope.md` (live position contract, filter growth,
TestCursor wording); recipe `validation-recipes.md` (filter growth control).
AGENTS.md, project skills SKILL.md files and end-user docs need no change: the
documented examples and APIs are unchanged.

### User Decisions - 2026-10-31 (close-out)

6. B - Go live tail/EOF checks read the header from an mmap mapping without a
   system call, matching libsystemd's cost model; ReadAt keeps one header
   `pread`. The header-mapping design is presented for approval before code.
7. B - Directory readers keep the last returned entry current after a failed
   move, as libsystemd and single-file readers do, in Go and Rust within this
   SOW.
8. B - CLI query/follow parity is re-certified against the pinned v262 oracle
   before this SOW closes; the project manager provides an oracle build script
   under `.local/` for the user to run.

### Implementation - Decision 7 (directory getters after a failed move)

- Go `DirectoryReader` records the file index and file position of the last
  returned key (`rememberKeyPosition`) on merged, sequential and
  cursor-direction success. A failed move (`parkOnCurrentKey`, called from
  merged `best == nil` and `finishSequentialEnd`) restores that file to the
  key and keeps it current when the key still matches; otherwise it reports
  no current entry as before.
- Rust `DirectoryReader` mirrors it with `key_position: (index, entry offset)`
  and `park_on_current_key`, restoring through `Location::ResolvedEntry` and a
  row clear, as the existing step-error path does. The success path adds one
  `get_entry_offset` field read per step; Go adds one position copy.
- Tests: Go `TestDirectoryGettersAfterFailedMove` (sequential, unfiltered and
  filtered) and merged getter/reversal checks in
  `TestPositionContractMergedDirectory`; Rust
  `directory_getters_keep_last_entry_after_failed_move` and the merged checks
  in `merged_directory_boundaries_and_eof`.
- Existing Go `TestNativeFilterDirectoryCandidateModesAndMutation` asserted
  the old directory contract (no current row and `TestCursor` ending at EOF);
  it now expects the last row and a true cursor test, as libsystemd returns.
- Spec `product-scope.md`: file and directory getters keep the last returned
  entry after a failed move; directories report none only after a seek. The
  raw Rust `FilterExpr` snapshot rule is now stated in the spec and recipe.
- Validation after decision 7 (2026-10-31): Go `CGO_ENABLED=0 go test
  -count=1 ./...` and `CGO_ENABLED=1 go test ./cmd/...` pass; Rust `cargo test
  --workspace` passes; against host systemd 255, `run_live_matrix.py` 8/8,
  `run_directory_matrix.py` PASS, `run_mixed_directory_matrix.py` 42/42; the
  Go live probe still matches the libsystemd table.

### User Decisions - 2026-11-01 (6B design)

6.1 A - Go mmap readers read the live header from a dedicated mapping of the
  file start, owned by the accessor outside the window LRU (never evicted, not
  counted against `MaxWindows`, reported in stats, unmapped on close). ReadAt
  readers keep one header `pread` into a reused buffer.
6.2 A - The live check reads only `n_entries` and the tail entry-array fields
  (gated by the stored header size) straight from the bytes, without a full
  header parse or allocation; any difference still runs the full refresh.
8 - The user installs the missing oracle build dependency and runs
  `.local/sow0045-oracle/build_v262_oracle.sh`; the query matrix then runs
  against the printed oracle.

### Implementation - Decision 6 (Go live header without a system call)

- `readerAccessor.liveHeader()` became `liveEntryState(headerSize)`, returning
  only `n_entries` and the tail entry-array fields (`parseLiveEntryState`,
  gated by the stored header size, no allocation).
- Mmap mode: `rollingReaderAccessor.headerWindow` maps the file start through
  the existing backend `mapWindow` on the first live check, on Unix and
  Windows. It is outside the window LRU, adds to `MappedBytes`, `MapCount` and
  `UnmapCount`, and is unmapped in `close()`. ReadAt mode reads into the
  accessor's reused `headerBuf`.
- A changed count still runs `refreshEntryOffsets`, which re-reads and
  validates the full header with `pread` as before. The cheap check, like
  libsystemd, does not re-validate the signature or state of an unchanged
  header.
- Cross-builds: windows, darwin, freebsd, linux/386. `GOOS=windows go vet`
  reports a pre-existing `unsafe.Pointer` warning in the unchanged
  `reader_access_mmap_windows.go`.
- Test: `TestLiveEntryCheckObservesAppendWithoutMapping` (mmap and ReadAt,
  unfiltered and filtered) checks that the mmap header mapping exists after the
  first check and that ReadAt makes none, that repeated tail-backward and EOF
  checks map nothing more, that appends are observed, and that close
  balances maps and unmaps.
- Spec `product-scope.md` records the live-check cost model.

### Performance - after decisions 6 and 7 (2026-11-01)

Fixture `.local/sow-0159-20261015-i54ghwxo/corpus/regular/system.journal`,
`run_reader_regression_comparison.py --filter-suite`, baseline = HEAD export.
The host ran at load average ~55 on 112 cores, so single timings are noisy:
the full 228-case run flags 40 cases at 1.01-1.28x. Most flips sign between
runs (for example Rust snapshot forward sparse 0.915 before decision 7, 1.255
after, with no Rust file-path change between them). The cases flagged in both
full runs were rerun at 31 repetitions:

- Go `sdk-filter` live mmap backward `restart-first`: 1.4x before 6B, now
  1.012 [0.90, 1.07] in the full run and 1.035 [1.02, 1.05] focused. Callgrind
  (deterministic): baseline 57.73M instructions, candidate 57.96M (+0.4%,
  about 23 instructions per restart), with the header pread removed.
- Go facade backward `fresh-first`/`restart-first`: 1.02-1.03x focused, same
  code path as above.
- Rust forward `fresh-first`/`restart-first` showed a real 6-11%
  (+50 ns per filter install) from recording the filter source with a
  separate vector and a copy of every match. Fixed: `JournalFilter` now moves
  its converted match payloads into its own recorded source
  (`FilterSource`, `take_source`) instead of dropping them, so install adds no
  copy. After the fix: facade live `fresh-first` 1.009 (inconclusive), sdk
  live `restart-first` 1.011, sdk snapshot `fresh-first` 1.054, facade
  `restart-first` 1.07. Callgrind for facade snapshot forward
  `restart-first`: baseline 15.09M instructions, candidate 14.89M (-1.3%), so
  the remaining timing difference is not added work.
- Go `sdk-unique-state` snapshot: 1.002 focused (inconclusive).

Residual: Go live backward restart is about 23 instructions per restart
above the baseline. It is not attributed per function (`callgrind_annotate`
is unavailable on the host); the likely source is the new edge and
live-check bookkeeping. No measured Rust restart case executes more
instructions than the baseline.

### User Decisions - 2026-11-01 (9, 8 follow-up)

- Decision 9 = A: accept the Go residual of about 23 instructions per live
  backward restart (+0.4% instructions, 1.03x focused timing) as the cost of
  the append check and edge bookkeeping. No further profiling.
- Decision 8 follow-up: the user authorized the agent to install the
  `liblzma-dev` build dependency and build the v262 oracle under `.local/`.

- Validation after the Rust filter-source fix (2026-11-01): Rust `cargo test
  --workspace` passes (45 suites); against host systemd 255,
  `run_live_matrix.py` 8/8, `run_directory_matrix.py` PASS,
  `run_mixed_directory_matrix.py` 42/42.

### Validation - v262 CLI Re-Certification (decision 8, 2026-11-01)

- Oracle: `.local/sow0045-oracle/build_v262_oracle.sh` cloned `systemd/systemd`
  tag `v262`, verified commit `8cc40e0c5e9234bf45084751ac53b1fbfe70b492`, and
  built only `journalctl` under `.local/`. `--version` reports
  `systemd 262 (262)` with `+PCRE2 +LZ4 +XZ +ZSTD`. The only host change was the
  user-authorized `liblzma-dev` package; host needrestart is list-only, so no
  service was restarted.
- `run_journalctl_query_matrix.py --journalctl <v262 oracle>`: status PASS,
  3,586 results PASS, 0 failures, including the `--follow` and cursor-file
  cases. CLI query/follow parity is re-certified on the final code, so the
  earlier "Not run" entry above is superseded.

### Final Whole-SOW Review - 2026-11-01

Scope: every change listed under the 2026-10-31 and 2026-11-01 implementation
sections, in Go and Rust, plus the spec and recipe edits.

- Behavior: file, directory and facade readers follow the libsystemd position
  table (decisions 1, 3, 7); appends after tail, EOF and installed filters are
  observed exactly once (decisions 2, 5); realtime-search ENTRY errors are
  returned (decision 4). The full test suites and the 576-cell contract
  matrix pass in both languages.
- Parity: Go and Rust implement the same boundary, park-on-last-key and filter
  rebuild rules; the shared interoperability matrices pass for Go, Rust and
  stock readers.
- Live header (decision 6): the Go mmap header mapping is created once, kept
  out of the window LRU, counted in stats and unmapped on close. A file shorter
  than the full header limits the mapping to the file size;
  `headerContainsField` checks the slice length, so the tail fields then read
  as zero and a later full refresh still validates the real header. An
  unchanged header is not re-validated, as in libsystemd. SIGBUS exposure
  equals that of the existing data windows (a journal is never truncated below
  its header). Windows views of the same file mapping stay coherent with
  writer updates.
- Directory park (decision 7): when the parked file cannot be restored to the
  last key, the reader reports no current entry and keeps the key as the
  location, matching the filtered behavior before this SOW.
- Rust filter source: converted match payloads move into the recorded source
  only after a successful conversion; a failed install resets the filter, so no
  stale source survives.
- Performance: Rust restart cases execute no more instructions than the
  baseline; the Go residual of about 23 instructions per live backward restart
  is accepted under decision 9.
- Security and side effects: no new unsafe code, subprocesses or host probes
  in SDK paths; the oracle build and fixtures stay under `.local/`.
- Formatting: `git diff --check`, `gofmt -l` and `rustfmt --edition 2024
  --check` on the changed Rust files are clean.

No blocking findings remain.

### Close-Out - 2026-11-01

Artifact maintenance:

- Spec `.agents/sow/specs/product-scope.md`: live position contract, filter
  growth, directory getters after a failed move, live-check cost model.
- Recipe `.agents/skills/project-journal-compatibility/references/validation-recipes.md`:
  raw Rust `FilterExpr` snapshot control.
- `.agents/sow/SOW-status.md`: SOW-0045 moved to completed; SOW-0071 is next.
- `AGENTS.md`, project `SKILL.md` files and end-user docs (`README`, `docs/`,
  `go/API.md`): no change needed, because the public APIs, examples and
  workflows are unchanged. The behavior changes are libsystemd-compatible
  position semantics, now stated in the spec.

Follow-ups: none. The pre-existing `GOOS=windows go vet` `unsafe.Pointer`
warning in the unchanged `reader_access_mmap_windows.go` and the
pre-existing workspace rustfmt/clippy findings are outside this SOW, and
neither is gated by CI. The v262 oracle is a `.local/` build and no longer
blocks validation.

Sensitive data: synthetic fixtures and identities only; benchmark and matrix
logs stay under `.local/`.
