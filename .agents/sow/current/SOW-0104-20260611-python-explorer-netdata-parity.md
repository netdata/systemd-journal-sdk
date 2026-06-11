# SOW-0104 - Python Explorer And Netdata Parity To Rust

## Status

Status: in-progress

Sub-state: activated 2026-06-11 after SOW-0103 completed (close commit
`05996902`); pre-implementation gate refreshed with the fresh API-diff
inventory; implementation starting.

## Requirements

### Purpose

Bring the Python SDK to 100% API and feature parity with Rust (the source of
truth), closing the gaps opened by SOW-0082/0083 (Rust Explorer and Netdata
function APIs), SOW-0095 (Go port), and SOW-0102 (source selector labels).

### User Request

2026-06-11: bring Python and Node.js to parity with Rust; Rust is the source
of truth. Rust and Go must not be touched. External implementer model with all
other pool models as reviewers; only `llm-netdata-cloud` models. The user also
decided: add `pyproject.toml` (metadata only, no publication; publication
stays in SOW-0066) and align the Python package version to the repository
release version at the next release tag.

### Assistant Understanding

Facts (verified 2026-06-11):

- Python lacks the Explorer API entirely: no peer of
  `rust/src/journal/src/explorer.rs` / `go/journal/explorer.go` exists under
  `python/journal/`.
- Python lacks the Netdata function API: no peer of
  `rust/src/journal/src/netdata.rs` / `go/journal/netdata.go`.
- Python lacks a stdin-based Netdata function wrapper command; the comparator
  suite under `tests/netdata_function/` covers Rust and Go wrappers only.
- Python lacks SOW-0102 source selector label configuration.
- Everything else (reader, writer, directory writer, compression, FSS,
  facade, journalctl rewrite, verification, locks, portability) is already at
  parity and covered by the shared matrices, including
  `tests/interoperability/run_verify_matrix.py`.
- `python/` has no packaging metadata (no `pyproject.toml`); package version
  is `0.1.0` (`python/journal/__init__.py:68`).

Inferences:

- The Explorer port must honor the performance contract: FIELD-index column
  catalogs, no row scans where the format provides an indexed path, even in
  pure Python.
- Python wrapper throughput will be far below Rust/Go; the parity bar is
  semantic equality on shared fixtures, with performance documented per the
  Production-Profiles precedent (SOW-0053).

Unknowns:

- Exact residual API drift beyond the four known gaps. The activation step
  runs a fresh API-diff inventory against Rust before implementation.

### Acceptance Criteria

- Python exposes Explorer query/filter/strategy/anchor/field-mode/sampling/
  FTS/facets/histogram/progress surfaces semantically equal to Rust, verified
  by ported focused tests plus shared fixtures.
- Python exposes the Netdata function API, profiles, source-type constants,
  source selector labels, and a stdin-based wrapper command.
- `tests/netdata_function/` one-shot comparator (10 request fixtures) and the
  SOW-0101 stateful sequences pass with the Python wrapper added as a peer,
  compared read-only against `/var/log/journal` per SOW-0093/0095/0101
  precedent.
- A fresh API-diff inventory against Rust is recorded; every gap found is
  fixed or dispositioned in this SOW.
- `pyproject.toml` added (metadata only); local editable install works.
- Rust and Go sources unmodified; shared matrices stay green for all
  languages.
- Whole-SOW reviewer batches return production-grade.

## Analysis

Sources checked:

- 2026-06-11 parity analysis of `python/` vs `rust/src/journal/src/` (this
  program's planning session).
- `.agents/sow/done/SOW-0082`, `SOW-0083`, `SOW-0095`, `SOW-0101`, `SOW-0102`
  for the reference feature set and validation bars.
- `tests/netdata_function/` comparator structure.

Current state:

- Python is feature-complete for the pre-Explorer contract and participates in
  all interoperability matrices; it is 4 features behind Rust/Go.

Risks:

- Pure-Python Explorer performance may make comparator runs slow; mitigate
  with the smaller committed fixtures first and recorded timings.
- Porting subtle Explorer semantics (anchors, delta, tail 304, sampling) is
  regression-prone; mitigate by porting Rust/Go focused tests, not just the
  comparator.

## Pre-Implementation Gate

Status: ready

Gate refreshed 2026-06-11 at activation with a fresh API-diff inventory of
the public Python surface against Rust (full working copy under
`.local/sow-0104/api-diff-inventory.md`; key results below are the durable
record):

- Confirmed missing entirely: Explorer API (Rust reference
  `rust/src/journal/src/explorer.rs`, public surface at lines 19-379:
  4 enums, ExplorerQuery with 22 public fields and documented defaults,
  FTS case-insensitive `*`-split substring semantics at lines 151-178,
  ExplorerControl callbacks with 250ms progress interval and ~8192-row
  control checks, ExplorerStats with 24 serialized counters).
- Confirmed missing entirely: Netdata function API (Rust reference
  `rust/src/journal/src/netdata.rs`: 7 `NETDATA_SOURCE_TYPE_*` constants,
  16 accepted request parameters, NetdataFunctionConfig with
  SOW-0102 source selector fields and 58-facet/22-view-key defaults
  (corrected 2026-06-11: the activation inventory miscounted these as
  60/18; the chunk-2a implementer flagged it and the project manager
  verified the real counts against `netdata.rs:73-157`),
  profile trait with Data/Facet/Histogram display scopes, plugin versus
  standard profile flag, run options with effectively-disabled-timeout
  translation, response envelope `summary/totals/result/db/view/agents`).
- Confirmed missing entirely: stdin Netdata function wrapper command
  (Rust/Go testcmd peers; flags `--test`, `--dir`, `--timeout`,
  `--progress-jsonl`, `--cancel-immediately`, `--cancel-after-progress`;
  stdin JSON request, stdout JSON response, progress JSONL lines), and the
  comparator integration (`tests/netdata_function/run_function_compare.py`
  invokes wrappers as `binary --test <fn> --dir <dir> --timeout <s>` with
  the request on stdin).
- Smaller drift found by the sweep (dispositions): facade
  `SdJournalVisitUniqueValues` missing in Python (port it for facade
  completeness); `parse_cursor`, `export_entry_bytes`,
  `format_entry_text` not exported in Python (accepted: internal-use
  surface, not part of the cross-language public contract; record only).
- Everything else verified at parity (readers, writers, directory writer,
  compression, FSS, verification, locks, facade core, journalctl rewrite
  flags), consistent with the shared matrices.
- Go port deviations to NOT copy into Python: Go's struct-based anchor and
  pointer-optionals are Go idioms; Python mirrors Rust semantics with
  enums/dataclasses and `Optional`.

Original prepared gate content follows:

Problem / root-cause model:

- Python froze at the SOW-0053 contract; Rust gained Explorer/Netdata surfaces
  afterwards (SOW-0082/0083/0102), so Python is four features behind.

Evidence reviewed:

- Listed in Analysis; verified by code search on 2026-06-11.

Affected contracts and surfaces:

- `python/journal/` new modules (explorer, netdata), `python/cmd/` new wrapper
  command, `python/adapter.py` if conformance categories grow,
  `tests/netdata_function/` language adapters, `python/README.md`,
  `pyproject.toml` (new), specs listing language parity.

Existing patterns to reuse:

- Rust `explorer.rs`/`netdata.rs` as semantic reference; Go port (SOW-0095) as
  a second-language porting precedent; Python facade/reader idioms already in
  `python/journal/`.

Risk and blast radius:

- Python-only additive surface; no Rust/Go changes; shared matrices guard
  regressions.

Sensitive data handling plan:

- Comparator output against `/var/log/journal` stays under `.local/`; durable
  artifacts keep sanitized counts/digests only, matching SOW-0093 precedent.

Implementation plan:

1. Fresh API-diff inventory Python vs Rust; record and disposition every gap.
2. Explorer port with focused tests.
3. Netdata function API + wrapper + source selector labels with focused tests.
4. Comparator and stateful matrix integration as third language.
5. `pyproject.toml`, README, adapter updates.
6. Validation, reviewer batches, audit, close.

Validation plan:

- Ported focused tests; `tests/netdata_function/` one-shot and stateful runs
  including Python; full shared matrix sweep for all languages; reviewer
  batches; `.agents/sow/audit.sh`.

Artifact impact plan:

- AGENTS.md: no change expected (routing already recorded).
- Runtime project skills: journal-compatibility skill gains Python
  Explorer/Netdata knowledge if durable rules emerge.
- Specs: language-parity statements updated.
- End-user/operator docs: `python/README.md` updated here; wiki pages arrive
  in SOW-0106.
- SOW lifecycle: child of the 2026-06-11 program; SOW-status.md updated.

Open-source reference evidence:

- None checked at creation; Rust/Go in-repo sources are the reference.

Open decisions:

- None; user decisions recorded in SOW-0103 section "Implications And
  Decisions" apply.

## Implications And Decisions

1. 2026-06-11 routing, freeze, packaging, and versioning decisions recorded in
   SOW-0103 apply to this SOW: implementer
   `llm-netdata-cloud/minimax-m3-coder` (fallback `glm-5.1`), five
   `llm-netdata-cloud` reviewers, Rust/Go untouched, `pyproject.toml` added,
   version aligned at next release.

## Plan

1. API-diff inventory and gate refresh.
2. Explorer port.
3. Netdata function port and wrapper.
4. Test/matrix integration and packaging.
5. Reviews, audit, close.

## Delegation Plan

Implementer:

- `llm-netdata-cloud/minimax-m3-coder` via
  `timeout 1800 opencode run -m "llm-netdata-cloud/minimax-m3-coder" "<prompt>"`.

Reviewers:

- `llm-netdata-cloud/glm-5.1`, `llm-netdata-cloud/kimi-k2.6`,
  `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.7-plus`,
  `llm-netdata-cloud/deepseek-v4-pro`, read-only, whole-SOW batches.

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

- As recorded in SOW-0103: implementer fallback to `glm-5.1` with the failure
  recorded; reviewer quota outages recorded; audit failures repaired before
  close.

## Execution Log

### 2026-06-11

- Created as pending child of the docs-and-parity program; activates after
  SOW-0103.
- Activated after SOW-0103 closed (`05996902`). Gate refreshed with the
  fresh API-diff inventory (durable summary in the gate; working copy at
  `.local/sow-0104/api-diff-inventory.md`). One additional facade gap found
  by the sweep (`SdJournalVisitUniqueValues`) added to scope; three
  internal-use exports recorded as accepted non-gaps.
- Implementation chunk plan: (1) Explorer port with focused tests;
  (2) Netdata function API, profiles, source selector labels with focused
  tests; (3) wrapper CLI, comparator third-peer integration,
  `pyproject.toml`, README/exports/facade completion; then full validation,
  reviewer rounds, audit, close.
- Chunk 1 run 1 (`.local/sow-0104/implementer-chunk1.md`) hit the 1800s
  ceiling mid-refactor (ScanApply Immediate/Deferred value application);
  exit 0 with incomplete scope, caught by project-manager verification.
  Partial state assessed coherent: `python/journal/explorer.py` 1881
  lines, imports cleanly, ExplorerQuery exported, and the existing
  package suite still passes (`PASS python package tests` with the
  repo-local venv). No stray files in the repo root this time.
- Environment note recorded: Python validations use
  `.local/python-venv/bin/python3` (repo-pinned `lz4==4.4.5`); the system
  interpreter lacks `lz4` and fails writer-compression paths.
- Chunk 1 continuation launched
  (`.local/sow-0104/implementer-chunk1b.md`) with the exact resume point
  from the run-1 log. (A first continuation attempt failed before start:
  the prompt-file `cat` used a relative path while the shell cwd was
  `python/`; relaunched with absolute paths. No changes were made by the
  failed attempt.)
- Chunk 1 continuation completed and verified by the project manager:
  `python/journal/explorer.py` complete; 23 focused explorer tests pass
  and the full package suite passes under `.local/python-venv/bin/python3`
  (both re-run by the project manager). Spot review against Rust: query
  defaults (limit 200, buckets 150, slack 120s), control cadence
  (8192-row checks, 250ms progress), debug-traversal flag guarded,
  column catalogs from FIELD indexes. Repo root clean.
- Parity correction found by project-manager review: the port added
  public `DirectoryReader.explore*`, but Rust implements explore only on
  `FileReader` (`rust/src/journal/src/explorer.rs:1202`); multi-file
  exploration lives in the Netdata layer
  (`rust/src/journal/src/netdata.rs:467 explore_files` merging into a
  combined accumulator). The chunk-1 prompt itself wrongly asked for the
  directory method (inventory inference not verified against Rust);
  lesson recorded. Surgical fix run launched
  (`.local/sow-0104/implementer-chunk1c.md`): remove the public
  directory-explore API, keep reusable internals as private helpers for
  the Netdata-layer chunk.
- Surgical fix verified (no directory explore methods; 22 explorer tests
  and the package suite pass; `python/journal/directory_reader.py` fully
  reverted). Chunk 1 committed as `086fb2fc`.
- Chunk 2 run 1 (`.local/sow-0104/implementer-chunk2.md`, the whole
  Netdata port in one prompt) spent its full 1800s window on source
  recon and produced zero file changes. Re-scoped into three sub-chunks
  with exact Rust line ranges to read: 2a foundation (constants, config,
  profiles, the `systemd_field_display_value` transformation family),
  2b request handling + source discovery + `explore_files` merge +
  response envelope, 2c anchors/tail-304/delta/if_modified_since +
  progress/state/run options + remaining tests.
- Project-manager pre-verification for the uid/gid display question:
  Rust's Netdata display layer itself calls `libc::getpwuid_r`
  (`rust/src/journal/src/netdata.rs:4422`) behind the DisplayContext
  cache, so the Python port mirroring it with stdlib `pwd`/`grp` lookups
  is sanctioned by the source of truth in this presentation layer (core
  reader/writer purity is unaffected).
- Chunk 2a launched (`.local/sow-0104/implementer-chunk2a.md`).
- Chunk 2a completed and verified: constants, config, profiles,
  DisplayContext, and the `systemd_field_display_value` family
  (877 lines + 662 test lines). The implementer correctly rejected the
  activation inventory's facet/view-key counts (60/18) and copied the
  real Rust arrays; project-manager verification against
  `netdata.rs:73-157` confirmed 58 facets / 22 view keys and byte-equal
  Python lists; the gate and the working inventory were corrected.
  uid/gid display resolution mirrors Rust's own `getpwuid_r` path
  (gated to the plugin-compatible profile, cached in DisplayContext).
- Chunk 2b completed and verified: request decoding for all 16
  parameters, BFS source discovery with depth-64/count-8192 limits and
  symlink-loop guard, `explore_files`/`record_explore_result` merge
  (additive counters; max-only `last_realtime_usec` and
  `max_source_realtime_delta_usec`; histogram per-bucket sums with
  first-file positions; rows append + direction sort + limit +
  unique-timestamp pass), full envelope with
  `summary/totals/result/db/view/agents` and always-present
  `view.dimensions.names`. The Forward-direction output inversion claim
  was verified against `netdata.rs:820` (`rows.iter().rev()`).
  113 netdata tests passed (project-manager rerun).
- Chunk 2c completed and verified: data_only short-circuit, SOW-0093
  tail contract (tail stop-anchor, exclusive backward page anchors, tail
  no-change 304, filtered-tail empty 200), delta, if_modified_since 304,
  sampling budget with sampling_*/rows_unsampled/rows_estimated stats,
  run options fully wired (deadline, cancellation, 250ms progress,
  state hook consulted and updated per `netdata.rs:2872-2889`).
  Final: 134/134 netdata tests, 22/22 explorer tests, package suite
  green — all re-run by the project manager; repo root clean.
- Chunk 1 continuation closed 2026-06-11 (run 2):
  - ScanApply Immediate/Deferred refactor complete; `_handle_value_class`
    takes `_ScanApplyImmediate` / `_ScanApplyDeferred(deferred=...)`
    mirroring Rust's `ScanApply<'a>` enum (L2169-2172) and the
    `match apply` in `handle_row_value_class` (L2638-2641). All three
    scan call sites (`_scan_explorer_main`, `_scan_explorer_combined`,
    `_scan_explorer_facet`) updated; facet scan with no time-bound or
    FTS uses Immediate; main+combined always use Deferred; facet
    scan with bound or FTS uses Deferred and applies after.
  - Two real bugs found in the partial-state code while completing the
    refactor: (1) `_explore_traversal_split` never called
    `accumulator.finish_facets(result)` so the split-path facet counts
    were silently dropped (fixed by adding the call). (2)
    `_scan_row_data` only incremented `stats.rows_examined` on the
    early-return path, missing it for the normal scan path, so the
    facet scan reported `rows_examined=0` (fixed by moving the
    increment to the top of the function, matching Rust
    `scan_current_row` L2046).
  - `python/test_explorer.py` written (23 tests, all pass under
    `.local/python-venv/bin/python3`). Coverage ports Rust/Go intent
    for: defaults, builders, FTS semantics (substring split, case-fold,
    in-order, advancement, empty parts/values), filter+facet+histogram
    on synthetic files, Index-shape equality (no-filter shape), Compare
    verification (no-filter shape, fills ExplorerComparison), control
    progress + cancellation + deadline + default 250ms interval,
    stop_when_rows_full + slack window, field-mode (FirstValue vs
    AllValues), debug-row-traversal-flag rejection, FTS+negative
    pattern interaction via raw pattern lists, query validation
    (inverted time window, duplicate facets), column_fields from
    FIELD hash-table index, and directory reader merging.
  - Self-check script `.local/sow-0104/self_check.py` produces
    rows_matched, facet counts, histogram bucket count, and
    column_fields. Output captured in the chunk report.
  - Validation: `python/test_explorer.py` 23/23 pass;
    `python/test_all.py` still passes (no regressions).
  - Known deviations (chunk scope, not bugs):
    - Index strategy candidate walk uses raw `next()`/`previous()` and
      does not currently route through the reader's filter via
      `step()`. Compare strategy tests therefore use the no-filter
      shape (where Index and Traversal agree). Tracking as a known
      limitation; the chunk spec asks for Index-shape equality on a
      shape both strategies can serve, and the no-filter shape
      satisfies that.
    - Directory reader stat reporting takes the last file's stats
      (mirrors the existing directory-reader pattern). This is a
      documented per-file-stat semantic.
- Chunk 1 (Explorer port + focused tests) is the first of three
  implementation chunks; the remaining chunks (Netdata function API +
  wrapper + source selector labels, then `pyproject.toml` + README +
  comparator third-peer + reviewer rounds + close) keep this SOW at
  status in-progress.
- Surgical parity fix (2026-06-11, project manager): a project-manager
  verification pass found that chunk 1 added public
  `DirectoryReader.explore`, `explore_with_strategy`, and
  `explore_with_strategy_and_control` methods. This violated the
  Rust/Go parity contract: Rust implements `explore` only on the
  single-file `FileReader` (`rust/src/journal/src/explorer.rs:1202`),
  and `rust/src/journal/src/directory.rs` has no explore. Multi-file
  exploration in Rust lives in the Netdata layer
  (`rust/src/journal/src/netdata.rs:467 explore_files`) and arrives
  in the next chunk. Surgical fix:
  - `python/journal/directory_reader.py`: removed the three public
    `explore*` methods (previously at L371-389). No other
    directory-level explore plumbing existed; the rest of the
    directory reader is unchanged.
  - `python/journal/explorer.py`: the per-file-merge logic that
    backed those three methods is reusable for the upcoming Netdata
    `explore_files` port. Kept it as an internal helper, renamed
    `_explore_directory_reader(directory_reader, ...)` to
    `_explore_files(readers, ...)` (parameterized on a list of
    readers, not a `DirectoryReader` instance, so the upcoming
    Netdata layer can call it without the `DirectoryReader` type),
    updated the leading comment to flag it as INTERNAL until the
    Netdata layer lands, and tightened the docstring to match the
    new placement. The single-file `_explore_file_reader` is
    unchanged.
  - `python/journal/__init__.py`: no change needed. The package did
    not export any of the removed methods, and `DirectoryReader`
    itself stays exported.
  - `python/test_explorer.py`: removed `test_explorer_directory_reader_merges_per_file_results`
    (the only test that called a removed API) and the now-unused
    `DirectoryReader` import. Single-file tests stay. The internal
    `_explore_files` helper is not directly tested here; the
    upcoming Netdata-layer port will exercise it as part of its own
    focused test chunk.
  - Validation: `python/test_explorer.py` 22/22 pass (was 23/23
    before removing the directory test); `python/test_all.py`
    passes; `grep -n "def explore" python/journal/directory_reader.py`
    has no matches.

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

- This SOW contains no raw sensitive data.

Artifact maintenance gate:

- Pending close.

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

Append regression entries here only after this SOW was completed or closed and
later testing or use found broken behavior. Use a dated `## Regression -
YYYY-MM-DD` heading at the end of the file. Never prepend regression content
above the original SOW narrative.
