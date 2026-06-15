# SOW-0097 - Go Codacy Metric Debt Refactor

## Status

Status: open

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: opened from SOW-0096 Codacy file metrics audit; refreshed on
2026-06-15 with current local/Codacy evidence and read-only subagent analysis;
blocked on user priority decision before implementation.

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

### Assistant Understanding

Facts:

- `.agents/sow/specs/codacy-rust-go-metrics-audit.md` classifies Go production
  files using Codacy file metrics plus local Lizard max function CCN. The spec
  is a 2026-06-07 point-in-time snapshot and must not be used as activation
  evidence without refresh.
- The 2026-06-15 refresh checked Codacy branch `master`, fetched at
  `2026-06-15T17:28:26.099Z`, with `222` Rust/Go files in the export.
- Current Go production evidence:
  - `go/journal/netdata.go`: `4162` lines, local Lizard `247` functions,
    sum CCN `924`, max CCN `13` at `netdataRequest.toExplorerQuery`
    (`go/journal/netdata.go:550`), Codacy duplication `0`, grade `C`.
  - `go/journal/explorer.go`: `2916` lines, local Lizard `186` functions,
    sum CCN `772`, max CCN `12` at `shouldStopWhenRowsFull`
    (`go/journal/explorer.go:2662`), Codacy duplication `111`, grade `F`.
  - `go/cmd/journalctl/main.go`: `1033` lines, local Lizard `62`
    functions, sum CCN `302`, max CCN `12` at `id128StringValid`
    (`go/cmd/journalctl/main.go:919`), Codacy duplication `71`, grade `F`.
  - `go/journal/verify_graph.go`: `1083` lines, local Lizard `46`
    functions, sum CCN `308`, max CCN `17` at `parseEntry`
    (`go/journal/verify_graph.go:543`), Codacy duplication `28`, grade `B`.
  - `go/journal/directory_reader.go`: `980` lines, local Lizard `75`
    functions, sum CCN `261`, max CCN `12` at `stepMerged`
    (`go/journal/directory_reader.go:515`), Codacy duplication `101`,
    grade `F`.
- Current local evidence invalidates the older statement that no tracked
  Rust/Go function exceeded local CCN `12`: `toExplorerQuery` is `13` and
  `parseEntry` is `17`.
- Concrete true duplication exists in journal file discovery helpers across
  `go/journal/directory_reader.go:133-223` and
  `go/cmd/journalctl/main.go:857-948`, including `isJournalSubdirName`,
  `id128StringValid`, and ASCII hex validation.

Inferences:

- The Go metric problem remains mostly file-size and responsibility pressure,
  but it now also includes two function-level complexity hotspots.
- The clearest true duplication fix is a shared internal journal filesystem
  helper used by the SDK directory reader and file-backed `journalctl`.
- Splitting large Go files improves ownership and per-file Codacy metrics, but
  does not reduce total project complexity unless duplicated logic is removed
  or high-CCN functions are decomposed.
- Explorer and Netdata splits are behavior-sensitive because they sit on recent
  sampling, facet, histogram, and Netdata integration parity work.

Unknowns:

- Whether Codacy grades improve enough after structural splits to justify the
  churn, especially when total complexity is redistributed rather than removed.
- Whether the shared journal filesystem helper should be exported from
  `go/journal` or remain private under `go/internal/`.
- Whether `parseEntry` diagnostics must remain byte-for-byte identical when
  decomposed.

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
- `.local/codacy/file-metrics-rust-go.validation.json` as scratch evidence.
- `.local/codacy/lizard-rust-go.csv` as scratch local CCN evidence.
- `.local/codacy-validity/current-file-metrics-rust-go.json` as refreshed
  2026-06-15 Codacy evidence.
- `.local/codacy-validity/lizard-sow-0097-0098.csv` as refreshed 2026-06-15
  local Lizard evidence.
- Read-only explorer subagent `019ecc60-e6a8-7223-b371-7450e5ac1a5e`
  completed on 2026-06-15 and returned concrete Go refactor candidates,
  estimated metric movement, risks, and validation scope.

Current state:

- Go still has production file ownership pressure in the SDK Explorer,
  directory reader, verifier, file-backed `journalctl`, and Netdata function
  wrapper surfaces.
- The older SOW-0096 statement that no function exceeded max CCN `12` is stale:
  `go/journal/netdata.go:550` is CCN `13`, and
  `go/journal/verify_graph.go:543` is CCN `17`.
- Some duplication is real shared-helper debt, especially directory/subdirectory
  journal discovery helpers duplicated between SDK and CLI code.

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
  validation behavior if entry item parsing is not preserved exactly.

## Pre-Implementation Gate

Status: needs-user-decision

Problem / root-cause model:

- Codacy file complexity is high because several Go files own large API,
  query, verification, or compatibility surfaces. Current local Lizard evidence
  also shows two function-level hotspots: `netdataRequest.toExplorerQuery`
  (`go/journal/netdata.go:550`, CCN `13`) and `graphVerifier.parseEntry`
  (`go/journal/verify_graph.go:543`, CCN `17`).

Evidence reviewed:

- `.agents/sow/specs/codacy-rust-go-metrics-audit.md`: historical Go top
  complexity and duplication tables from 2026-06-07.
- `.local/codacy-validity/current-file-metrics-rust-go.json`: refreshed
  2026-06-15 Codacy export.
- `.local/codacy-validity/lizard-sow-0097-0098.csv`: refreshed local Lizard
  function metrics for the target files.
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

1. Long-term-best: shared internal journal filesystem helper.
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
2. Surgical: split `go/cmd/journalctl/main.go` by CLI responsibility.
   - Candidate slices: flags/dispatch (`109-325`), timestamp parsing
     (`327-499`), boot logic (`501-619`), query/follow output (`621-757`),
     verify/key/directory handling (`759-1033`).
   - Estimated improvement: move about `750-900` lines out of `main.go`,
     likely reducing the entry file below `200` lines. True complexity drops
     only when paired with shared-helper extraction.
3. Surgical: split `go/journal/netdata.go` by Netdata function ownership.
   - Candidate slices: request/query conversion (`416-672`), page window and
     realtime adjustment (`673-965`), file exploration and merge (`966-1323`),
     response/facet/histogram assembly (`1324-1920`), source summaries and file
     selection (`1921-2603`), display/column helpers (`2708-2820`,
     `3376-4034`).
   - Estimated improvement: move `3000+` lines out of `netdata.go`; strong
     per-file Codacy improvement, but little true total-complexity reduction.
4. Surgical only if behavior is unchanged: split `go/journal/explorer.go` by
   Explorer engine layer.
   - Candidate slices: public query/control/result types (`13-403`), sampling
     (`404-731`), strategy orchestration (`731-938`), indexed FIELD/DATA paths
     (`942-1333`), accumulator/cache (`1340-1693`), scan loops/classification
     (`1694-2228`), FTS/time/histogram/compare helpers (`2229-2905`).
   - Estimated improvement: move `2500+` lines out of `explorer.go`; true
     complexity reduction is low unless duplicated logic is also removed.
5. Long-term-best: verifier parse-entry CCN reduction.
   - Decompose `go/journal/verify_graph.go:543-610` and share compact versus
     regular entry item reading with `go/journal/verify_graph.go:655-700`.
   - Candidate helpers: entry item size, entry header reader, offset item
     reader, and entry item validator.
   - Estimated improvement: max CCN for `parseEntry` should fall from `17` to
     about `7-10`; file-level complexity may not drop much.

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

1. Ask the user to approve the first Go target and priority order.
2. Prefer true duplication removal first: shared journal filesystem helper.
3. Split by stable ownership boundaries, not by arbitrary line count.
4. Decompose high-CCN verifier code only with exact diagnostic/compact-file
   compatibility tests.
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
2. Which Go file group to tackle first.

## Implications And Decisions

User decision on 2026-06-15:

- Refresh this pending SOW with current evidence and read-only subagent
  findings.
- Record concrete solution candidates, estimated size or metric improvement,
  and refactor risks.
- No implementation, API change, behavior change, or commit was approved by
  this decision.

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

## Validation

Pending.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

Pending.
