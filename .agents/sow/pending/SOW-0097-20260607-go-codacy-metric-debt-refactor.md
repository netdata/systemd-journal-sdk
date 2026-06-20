# SOW-0097 - Go Codacy Metric Debt Refactor

## Status

Status: open

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: opened from SOW-0096 Codacy file metrics audit; refreshed again on
2026-06-21 with current local code evidence, Lizard metrics, and Codacy Cloud
repository-level evidence. Still open, but narrowed to real Go duplication and
ownership debt. Parked by user decision on 2026-06-21; no implementation is
planned before the integration/release backlog unless Go maintainability
pressure returns.

## Requirements

### Purpose

Reduce Go production complexity and duplication metrics only where doing so
improves maintainability without hurting journal reader/writer performance,
compatibility, or Netdata integration behavior.

### User Request

The user asked for Codacy file-by-file analysis of Rust and Go complexity and
duplication, with interest in whether indicators are reasonable. SOW-0096 found
real Go production file-size and ownership pressure that needs a dedicated
refactor decision.

On 2026-06-15, the user asked to check whether this SOW was still valid, then
to update it with current evidence, concrete solution candidates, estimated size
or metric improvement, and refactor risks from a read-only subagent analysis.
On 2026-06-21, the user asked to refresh SOW-0097 and SOW-0098 again and show
what remains to be done.

### Assistant Understanding

Facts:

- `.agents/sow/specs/codacy-rust-go-metrics-audit.md` classifies Go production
  files using Codacy file metrics plus local Lizard max function CCN. The spec
  is a 2026-06-07 point-in-time snapshot and must not be used as activation
  evidence without refresh.
- The 2026-06-15 refresh checked Codacy branch `master`, fetched at
  `2026-06-15T17:28:26.099Z`, with `222` Rust/Go files in the file-metric
  export. Treat those per-file Codacy grades as a historical baseline, not as
  current activation evidence.
- Codacy Cloud repository refresh on 2026-06-21 reports default branch
  `master`, last analysed commit `c9f5caac804f` with analysis ending
  `2026-06-19T06:57:32.973Z`, `27` open issue rows, coverage `72%`, complex
  files `18%`, and duplication `30%`. The current Cloud issue set is unrelated
  to this refactor SOW: it is the approved Go directive SCA cluster plus
  markdownlint rows.
- Current local Go production evidence from 2026-06-21:
  - `go/journal/netdata.go`: `4193` lines, Lizard `3906` NLOC,
    `252` functions, average CCN `3.7`.
  - `go/journal/explorer.go`: `2916` lines, Lizard `2681` NLOC,
    `186` functions, average CCN `4.2`.
  - `go/cmd/journalctl/main.go`: `1033` lines, Lizard `945` NLOC,
    `62` functions, average CCN `4.9`.
  - `go/journal/verify_graph.go`: `1117` lines, Lizard `1052` NLOC,
    `53` functions, average CCN `6.0`.
  - `go/journal/directory_reader.go`: `980` lines, Lizard `865` NLOC,
    `75` functions, average CCN `3.5`.
- Current local Lizard evidence invalidates the 2026-06-15 hotspot claim:
  `netdataRequest.toExplorerQuery` is now CCN `9`
  (`go/journal/netdata.go:550`), and `graphVerifier.parseEntry` is now CCN
  `5` (`go/journal/verify_graph.go:543`). No targeted Go file exceeded
  Lizard's configured local threshold.
- The current highest local Go function CCNs in the target set are CCN `12`:
  `matchesSource` (`go/journal/netdata.go:518`),
  `shouldStopWhenRowsFull` (`go/journal/explorer.go:2662`),
  `id128StringValid` (`go/cmd/journalctl/main.go:919`),
  `parseEntryArray` (`go/journal/verify_graph.go:682`),
  `id128StringValid` (`go/journal/directory_reader.go:195`), and
  `stepMerged` (`go/journal/directory_reader.go:515`).
- Concrete true duplication exists in journal file discovery helpers across
  `go/journal/directory_reader.go:133-223` and
  `go/cmd/journalctl/main.go:857-948`, including `isJournalSubdirName`,
  `id128StringValid`, and ASCII hex validation.
- `go/journal/netdata.go:2435` still has a separate recursive Netdata journal
  collector. It is behavior-specific and should not be blindly merged with the
  systemd-style one-level journal directory traversal.

Inferences:

- The Go metric problem remains mostly file-size, responsibility pressure, and
  true duplicated journal discovery helpers. The earlier function-hotspot claim
  is stale.
- The clearest true duplication fix is a shared internal journal filesystem
  helper used by the SDK directory reader and file-backed `journalctl`.
- Splitting large Go files may improve ownership and per-file Codacy metrics,
  but it does not reduce total project complexity unless duplicated logic is
  removed or specific complex functions are simplified.
- Explorer and Netdata splits are behavior-sensitive because they sit on recent
  sampling, facet, histogram, and Netdata integration parity work.

Unknowns:

- Whether Codacy grades improve enough after structural splits to justify the
  churn, especially when total complexity is redistributed rather than removed.
- Whether the shared journal filesystem helper should be exported from
  `go/journal` or remain private under `go/internal/`.
- Whether any verifier cleanup should target `parseEntryArray` rather than the
  now-low-CCN `parseEntry`.

### Acceptance Criteria

- User-approved Go refactor target list and priority order are recorded before
  implementation.
- Refactors preserve public Go APIs unless a separate user decision allows API
  changes.
- Go unit, interoperability, journalctl, and benchmark smoke validation pass.
- Codacy file metrics are rechecked after push and compared against SOW-0096
  baseline.

## Analysis

Sources checked:

- `.agents/sow/specs/codacy-rust-go-metrics-audit.md`.
- `.local/codacy-validity/current-file-metrics-rust-go.json` as refreshed
  2026-06-15 Codacy file-metric baseline evidence.
- `.local/codacy-validity/lizard-sow-0097-0098.csv` as refreshed 2026-06-15
  local Lizard baseline evidence.
- 2026-06-21 current line counts from `wc -l`.
- 2026-06-21 targeted Lizard run over the Go target files.
- 2026-06-21 Codacy Cloud repository query for repo-level metrics and issue
  categories. Raw user/account metadata was not written to durable artifacts.
- 2026-06-21 `git log --oneline --since=2026-06-15 -- ...` over the Go target
  files showed commits `e17f694` and `b1f6a36`, so the 2026-06-15 local
  function metrics needed refresh.
- Read-only explorer subagent `019ecc60-e6a8-7223-b371-7450e5ac1a5e`
  completed on 2026-06-15 and returned concrete Go refactor candidates,
  estimated metric movement, risks, and validation scope.

Current state:

- Go still has production file ownership pressure in the SDK Explorer, file
  backed `journalctl`, verifier, directory reader, and Netdata function wrapper
  surfaces.
- The 2026-06-15 claim that `toExplorerQuery` and `parseEntry` were CCN `13`
  and `17` is stale. Current local Lizard reports CCN `9` and `5`.
- The real actionable duplication remains shared-helper debt: journal filename,
  one-level machine-id subdirectory traversal, ID128 validation, and ASCII-hex
  validation duplicated between SDK and CLI code.
- Large file splits are still possible, but they are readability and ownership
  work first. They should not be sold as true complexity reduction unless they
  remove duplication or simplify functions.

Risks:

- Splitting hot-path files can reduce Codacy file metrics while making runtime
  behavior harder to reason about if ownership boundaries are arbitrary.
- Refactoring the Explorer or Netdata wrapper before Netdata integration can
  destabilize recently validated parity, sampling, facet, histogram, and
  performance behavior.
- Shared filesystem helper work must preserve the difference between
  systemd-style one-level machine-id subdirectory traversal and Netdata's
  recursive depth/dedup/error-accounting scan.
- Verifier refactoring can change corruption diagnostics or compact-file
  validation behavior if entry-array parsing is not preserved exactly. Current
  evidence does not justify treating `parseEntry` itself as the main hotspot.

## Pre-Implementation Gate

Status: refreshed-needs-user-decision

Problem / root-cause model:

- Codacy file complexity is high because several Go files own large API,
  query, verification, or compatibility surfaces. Current local Lizard evidence
  no longer supports the earlier `toExplorerQuery` / `parseEntry` hotspot
  claim. The durable reason to keep this SOW open is true duplicated journal
  discovery helper logic plus large ownership surfaces.

Evidence reviewed:

- `.agents/sow/specs/codacy-rust-go-metrics-audit.md`: historical Go top
  complexity and duplication tables from 2026-06-07.
- `.local/codacy-validity/current-file-metrics-rust-go.json`: refreshed
  2026-06-15 Codacy file-metric baseline export.
- `.local/codacy-validity/lizard-sow-0097-0098.csv`: refreshed local Lizard
  function metric baseline for the target files.
- 2026-06-21 current line counts, current targeted Lizard, and Codacy Cloud
  repository-level state.
- Read-only explorer subagent `019ecc60-e6a8-7223-b371-7450e5ac1a5e`.

Affected contracts and surfaces:

- Go public SDK APIs.
- Go Explorer and Netdata function compatibility behavior.
- Go file-backed journalctl behavior.
- Go reader/writer benchmarks and interoperability tests.

Existing patterns to reuse:

- Existing Go package boundary under `go/journal/`.
- Existing focused files such as `writer_objects.go`, `writer_arrays.go`, and
  `writer_compression.go` as examples of split-by-format-responsibility.

Concrete solution candidates:

1. Long-term-best, recommended next action: shared internal journal filesystem
   helper.
   - Extract duplicated helpers from `go/journal/directory_reader.go:133-223`
     and `go/cmd/journalctl/main.go:857-948`.
   - Candidate package: `go/internal/journalfs`, unless implementation analysis
     proves an unexported `go/journal` helper is safer.
   - Include journal filename checks, ID128 string validation, journal
     subdirectory-name validation, and regular-file/directory probes.
   - Keep `go/journal/netdata.go:2404-2489` separate or option-driven because
     Netdata recursive scan behavior has depth, symlink, dedup, source metadata,
     and error-accounting semantics that are not the same as systemd-style
     one-level traversal.
   - Estimated improvement: `80-120` duplicated lines centralized; likely
     meaningful duplication drop in `go/journal/directory_reader.go` and
     `go/cmd/journalctl/main.go`; total project complexity mostly unchanged.
   - Current validity: still valid on 2026-06-21 and the clearest remaining
     work.
2. Surgical: split `go/cmd/journalctl/main.go` by CLI responsibility.
   - Candidate slices: flags/dispatch (`109-325`), timestamp parsing
     (`327-499`), boot logic (`501-619`), query/follow output (`621-757`),
     verify/key/directory handling (`759-1033`).
   - Estimated improvement: move about `750-900` lines out of `main.go`,
     likely reducing the entry file below `200` lines. True complexity drops
     only when paired with shared-helper extraction.
   - Current validity: optional. Do this for maintainability if the CLI keeps
     growing, not as the first Codacy-driven fix.
3. Surgical: split `go/journal/netdata.go` by Netdata function ownership.
   - Candidate slices: request/query conversion (`416-672`), page window and
     realtime adjustment (`673-965`), file exploration and merge (`966-1323`),
     response/facet/histogram assembly (`1324-1920`), source summaries and file
     selection (`1921-2603`), display/column helpers (`2708-2820`,
     `3376-4034`).
   - Estimated improvement: move `3000+` lines out of `netdata.go`; strong
     per-file Codacy improvement, but little true total-complexity reduction.
   - Current validity: defer unless maintainers want clearer Netdata wrapper
     ownership before integration work.
4. Surgical only if behavior is unchanged: split `go/journal/explorer.go` by
   Explorer engine layer.
   - Candidate slices: public query/control/result types (`13-403`), sampling
     (`404-731`), strategy orchestration (`731-938`), indexed FIELD/DATA paths
     (`942-1333`), accumulator/cache (`1340-1693`), scan loops/classification
     (`1694-2228`), FTS/time/histogram/compare helpers (`2229-2905`).
   - Estimated improvement: move `2500+` lines out of `explorer.go`; true
     complexity reduction is low unless duplicated logic is also removed.
   - Current validity: defer. Explorer is behavior-sensitive and current
     evidence does not show an urgent function-level hotspot.
5. Conditional surgical follow-up: verifier entry-array cleanup.
   - Do not target `parseEntry` as originally written in this SOW; current CCN
     is `5`.
   - If verifier readability becomes a priority, inspect `parseEntryArray`
     (`go/journal/verify_graph.go:682`, CCN `12`) and compact versus regular
     entry item reading.
   - Estimated improvement: modest. This is not a first-priority Codacy debt
     item after the 2026-06-21 refresh.

Risk and blast radius:

- Medium-to-high for Explorer/Netdata wrapper files because they are recent,
  Netdata-facing, and performance-sensitive.
- Medium for verifier/journalctl files because behavior is CLI/compatibility
  visible.
- Low-to-medium for the shared journal filesystem helper if exact traversal
  semantics and tests are preserved.

Sensitive data handling plan:

- Do not commit raw Codacy API exports. Durable artifacts may include file
  paths and numeric metrics only.

Implementation plan:

1. Ask the user to approve whether to implement this now or leave it pending
   until after Netdata integration work.
2. If implemented, start with the shared journal filesystem helper because it
   removes true duplication rather than only redistributing file complexity.
3. Split by stable ownership boundaries only if the user chooses readability
   work after the helper extraction.
4. Treat verifier work as optional and focused on `parseEntryArray`, not the
   stale `parseEntry` hotspot.
5. Run Go tests, interoperability smoke, benchmarks where hot paths changed,
   and Codacy metric recheck.

Validation plan:

- `cd go && GOCACHE="$PWD/.local/go-build" GOMODCACHE="$PWD/.local/go-mod-cache" go test ./...`.
- Targeted Go tests for touched surfaces, including directory reader,
  file-backed `journalctl`, Explorer, Netdata function, and verifier tests.
- `python3 tests/interoperability/run_directory_matrix.py` for directory
  traversal changes.
- `python3 tests/interoperability/run_journalctl_query_matrix.py` for CLI or
  file-backed `journalctl` changes.
- `python3 tests/interoperability/run_verify_matrix.py` for verifier changes.
- `python3 tests/benchmarks/run_reader_core_benchmarks.py` when Explorer or
  reader hot paths change.
- Codacy file metrics export after push.

Artifact impact plan:

- AGENTS.md: no expected update.
- Runtime project skills: no expected update unless a new Codacy refactor
  workflow becomes a repeated rule.
- Specs: update if public Go behavior changes; otherwise record metric outcome
  in this SOW.
- End-user/operator docs: update only if APIs or CLI help change.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: complete this SOW after implementation, review, validation,
  and remote Codacy evidence.
- SOW-status.md: update on activation and close.

Open-source reference evidence:

- None checked yet. This SOW is pending and blocked on user priority.

Open decisions:

1. Whether to refactor Go metric debt before Netdata integration or defer it
   until after integration performance gates.
2. If this SOW activates, whether the first target is the recommended shared
   journal filesystem helper.
3. Whether large Netdata/Explorer/CLI file splits are worth the churn now, given
   that current evidence shows ownership pressure but not urgent function-level
   threshold failures.

## Implications And Decisions

User decision on 2026-06-15:

- Refresh this pending SOW with current evidence and read-only subagent
  findings.
- Record concrete solution candidates, estimated size or metric improvement,
  and refactor risks.
- No implementation, API change, behavior change, or commit was approved by
  this decision.

User decision on 2026-06-21:

- Defer SOW-0097 and SOW-0098.
- Keep this SOW open in `pending/` as tracked quality debt, but do not treat it
  as mandatory or active work before SOW-0048, SOW-0049, SOW-0050, and SOW-0066.
- Revisit only if Go maintainability, Codacy duplication, or repeated edits to
  the duplicated journal filesystem helper area make the cleanup worth the
  churn.

## Plan

1. Decide priority and target file group.
2. If selected, implement the shared journal filesystem helper first because it
   removes true duplication rather than only redistributing file complexity.
3. Then choose one coherent file-ownership split or high-CCN function
   decomposition.
4. Validate behavior, performance, and Codacy metric movement.

## Delegation Plan

Implementer:

- Local implementation by the project manager unless the user changes routing.

Reviewers:

- Run the approved reviewer pool after the complete SOW implementation and
  local validation.

Repository boundary block for every external-agent prompt:

```text
CRITICAL REPOSITORY BOUNDARY:
- Do not make changes outside this repository for any reason.
- Repository path: current repository root.
- You may inspect external references read-only when the task requires it.
- Write, edit, delete, move, reset, checkout, install, generate, cache, or format nothing outside this repository.
- The only write exception outside the repository is /tmp.
- Prefer .local/ inside this repository for scratch work, generated temporary files, cloned references, logs, and working notes.
```

Failure handling:

- If refactor risk exceeds metric value, record evidence and ask the user before
  continuing.

## Execution Log

2026-06-15:

- Refreshed current Go metric evidence from local Codacy export and Lizard
  output under `.local/codacy-validity/`.
- Ran read-only explorer subagent
  `019ecc60-e6a8-7223-b371-7450e5ac1a5e` for Go SOW analysis.
- Updated this SOW with current evidence, solution candidates, estimated
  improvement, and risks.

2026-06-21:

- Refreshed this pending SOW again from current local code and Codacy Cloud
  repository-level state.
- Corrected stale 2026-06-15 function-hotspot claims: `toExplorerQuery` is CCN
  `9`, and `parseEntry` is CCN `5`.
- Narrowed remaining actionable work to duplicated Go journal filesystem helper
  extraction first, with large file splits and verifier cleanup treated as
  optional follow-ups.

## Validation

Refresh validation:

- Current line counts checked for the Go target files.
- Targeted Lizard run completed for the Go target files; no targeted Go file
  exceeded Lizard's configured local threshold.
- Codacy Cloud repository state checked read-only on 2026-06-21.
- No implementation, source behavior, tests, public docs, specs, or runtime
  skills changed by this refresh.

## Outcome

Open and parked by user decision. The SOW is still valid, but it is not
mandatory work. The recommended remaining implementation, if this is later
reactivated, is the shared journal filesystem helper extraction. Broad file
splitting should wait unless the user chooses a readability-maintenance pass.

## Lessons Extracted

Refresh lesson: point-in-time Codacy/Lizard statements age quickly in this
repository. Activation must re-run local metrics before treating an old
function-hotspot list as fact.

## Followup

No follow-up action is planned now. If activated later, implement SOW-0097
before SOW-0098 only if Go maintainability is the priority. Otherwise leave it
pending until after the Netdata integration and release SOWs.
