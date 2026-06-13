# SOW-0105 - Node.js Explorer And Netdata Parity To Rust

## Status

Status: in-progress

Sub-state: activated 2026-06-12 after SOW-0104 completed (close commit
`ee5d0d20`); gate refreshed with a fresh Node-vs-Rust API-diff inventory
(project-manager-verified); implementation starting with the SOW-0104
porting playbook inherited.

## Requirements

### Purpose

Bring the Node.js SDK to 100% API and feature parity with Rust (the source of
truth), closing the gaps opened by SOW-0082/0083 (Rust Explorer and Netdata
function APIs), SOW-0095 (Go port), and SOW-0102 (source selector labels), and
add TypeScript definitions per the user's 2026-06-11 decision.

### User Request

2026-06-11: bring Python and Node.js to parity with Rust; Rust is the source
of truth. Rust and Go must not be touched. External implementer model with all
other pool models as reviewers; only `llm-netdata-cloud` models. The user also
decided: add hand-written `.d.ts` TypeScript definitions with a CI type-check,
and align the Node package version to the repository release version at the
next release tag.

### Assistant Understanding

Facts (verified 2026-06-11):

- Node lacks the Explorer API entirely: no peer of
  `rust/src/journal/src/explorer.rs` exists under `node/src/`.
- Node lacks the Netdata function API: no peer of
  `rust/src/journal/src/netdata.rs`.
- Node lacks a stdin-based Netdata function wrapper command; the comparator
  suite under `tests/netdata_function/` covers Rust and Go wrappers only.
- Node lacks SOW-0102 source selector label configuration.
- Node has no TypeScript definitions; package `@netdata/systemd-journal-sdk`
  is pure-JS ESM at version `0.1.0`, Node `>=22.15.0`.
- Everything else is at parity and covered by the shared matrices.
- Constraint: no native addons; all three missing features are pure-JS
  implementable (no new compression or platform needs).

Inferences:

- The Explorer port must honor the performance contract (FIELD-index column
  catalogs, indexed paths over row scans) within the existing no-mmap
  positioned-read runtime model.
- Node wrapper throughput will be below Rust/Go; parity bar is semantic
  equality on shared fixtures with documented performance, per the SOW-0054
  precedent.

Unknowns:

- Exact residual API drift beyond the four known gaps. The activation step
  runs a fresh API-diff inventory against Rust before implementation.

### Acceptance Criteria

- Node exposes Explorer query/filter/strategy/anchor/field-mode/sampling/
  FTS/facets/histogram/progress surfaces semantically equal to Rust, verified
  by ported focused tests plus shared fixtures.
- Node exposes the Netdata function API, profiles, source-type constants,
  source selector labels, and a stdin-based wrapper command.
- `tests/netdata_function/` one-shot comparator and SOW-0101 stateful
  sequences pass with the Node wrapper added as a peer, compared read-only
  against `/var/log/journal` per SOW-0093/0095/0101 precedent.
- A fresh API-diff inventory against Rust is recorded; every gap found is
  fixed or dispositioned in this SOW.
- Hand-written `.d.ts` covers the public API; CI type-checks the definitions
  and the verified doc examples against them; no runtime TypeScript
  dependency is introduced.
- No native addon or runtime native-code loading is introduced.
- Rust and Go sources unmodified; shared matrices stay green for all
  languages.
- Whole-SOW reviewer batches return production-grade.

## Analysis

Sources checked:

- 2026-06-11 parity analysis of `node/` vs `rust/src/journal/src/` (this
  program's planning session).
- `.agents/sow/done/SOW-0082`, `SOW-0083`, `SOW-0095`, `SOW-0101`, `SOW-0102`
  for the reference feature set and validation bars; SOW-0054 and SOW-0072 for
  Node runtime and dependency constraints.
- `tests/netdata_function/` comparator structure.

Current state:

- Node is feature-complete for the pre-Explorer contract and participates in
  all interoperability matrices; it is 4 features behind Rust/Go plus the
  missing type definitions.

Risks:

- Whole-file Buffer reads could make large-directory Explorer queries
  memory-heavy; the port must follow the existing reader windowing/positioned
  read patterns and record measured behavior.
- `.d.ts` drift from the JS implementation; mitigated by CI `tsc` checks
  against the typed surface and the SOW-0106 verified examples.

## Pre-Implementation Gate

Status: ready

Gate refreshed 2026-06-12 at activation with a fresh Node-vs-Rust API-diff
inventory; inventory claims were verified against code by the project
manager before entering this gate (working copy under
`.local/sow-0105/api-diff-inventory.md`):

- Confirmed missing entirely (same four as Python at its activation):
  Explorer API, Netdata function API, stdin function wrapper, SOW-0102
  source selector labels — no explorer/netdata code under `node/src/`.
- Confirmed additional gap: the facade streaming unique-values visitor
  (`SdJournalVisitUniqueValues` peer) is absent in Node
  (`node/src/facade.js`; `queryUnique` exists at
  `node/src/lib/reader.js:506`, the visitor variant does not) — same
  gap class Python closed in SOW-0104.
- Inventory claims REFUTED by project-manager code verification and
  excluded from scope: "DATA decompression not implemented" (zstd
  whole-file path at `node/src/lib/reader.js:21,83`; per-DATA codecs in
  `compress.js`/`lz4-block.js`/`xz-block.js`, proven by the compression
  matrices since SOW-0054) and "FIELD index traversal missing"
  (`node/src/lib/reader.js:527-535` `_enumerateFieldsIndexed` with the
  documented entry-scan fallback, from SOW-0027).
- The inventory's "minimal JSDoc instead of .d.ts" suggestion is
  REJECTED: the user's recorded 2026-06-11 decision stands — hand-
  written `.d.ts` with CI type-check is in scope.
- SOW-0104 porting playbook inherited; the Rust mechanisms that cost
  Python the most rework are frontloaded into the chunk prompts with
  their verified line references: `normalize_time_window` /
  `relative_window_to_absolute` (`netdata.rs:3624/3658`, parse-time
  now-anchoring, relative small-magnitude rule, end-of-second
  rounding); `response_analysis_keys` delta keys (`netdata.rs:2613`)
  gated by `!data_only || delta` (`netdata.rs:2602`); synthetic
  `ND_JOURNAL_FILE` from the located file in row building; configured-
  profile threading and per-request DisplayContext; source-summary
  bounds from header-only reads; `accepted_params` filter-field
  extension; window-bounded column catalogs; `merge_histogram` strict
  bucket sums (`netdata.rs:2621`); tail+delta `skips_after` init
  (`netdata.rs:1903`); pre-scan-304 vs post-scan empty-200
  (`netdata.rs:2677`). The Python port is a secondary porting
  reference; Rust remains the authority on any conflict.
- Comparator integration: the runners take a `--python` third peer; the
  Node wrapper joins via the same pattern (`--node`/`--node-interpreter`
  or generalized peer flags — implementer follows the existing adapter
  shape; defaults unchanged).
- Runtime constraints for the port: pure ESM, no native addons, no
  mmap (whole-file Buffer reads and positioned access), `node:zlib`
  zstd, `lz4js`, vendored XZ WASM, Node >=22.15.0.

Original prepared gate content follows:

Problem / root-cause model:

- Node froze at the SOW-0054 contract; Rust gained Explorer/Netdata surfaces
  afterwards (SOW-0082/0083/0102), so Node is four features behind, and npm
  consumers lack type definitions.

Evidence reviewed:

- Listed in Analysis; verified by code search on 2026-06-11.

Affected contracts and surfaces:

- `node/src/` new modules (explorer, netdata), `node/cmd/` new wrapper
  command, `node/adapter/` if conformance categories grow, new `.d.ts` and CI
  type-check, `tests/netdata_function/` language adapters, `node/README.md`,
  `node/package.json` (types field; no publication).

Existing patterns to reuse:

- Rust `explorer.rs`/`netdata.rs` as semantic reference; Go (SOW-0095) and
  Python (SOW-0104) ports as porting precedents; Node facade/reader idioms in
  `node/src/lib/`.

Risk and blast radius:

- Node-only additive surface; no Rust/Go changes; shared matrices guard
  regressions.

Sensitive data handling plan:

- Comparator output against `/var/log/journal` stays under `.local/`; durable
  artifacts keep sanitized counts/digests only, matching SOW-0093 precedent.

Implementation plan:

1. Fresh API-diff inventory Node vs Rust; record and disposition every gap.
2. Explorer port with focused tests.
3. Netdata function API + wrapper + source selector labels with focused tests.
4. Comparator and stateful matrix integration as fourth language.
5. `.d.ts` authoring plus CI type-check wiring; README and package metadata.
6. Validation, reviewer batches, audit, close.

Validation plan:

- Ported focused tests; `tests/netdata_function/` one-shot and stateful runs
  including Node; full shared matrix sweep; `tsc` type-check job; reviewer
  batches; `.agents/sow/audit.sh`.

Artifact impact plan:

- AGENTS.md: no change expected.
- Runtime project skills: journal-compatibility skill gains Node
  Explorer/Netdata knowledge if durable rules emerge.
- Specs: language-parity statements updated.
- End-user/operator docs: `node/README.md` updated here; wiki pages arrive in
  SOW-0106.
- SOW lifecycle: child of the 2026-06-11 program; SOW-status.md updated.

Open-source reference evidence:

- None checked at creation; Rust/Go in-repo sources are the reference.

Open decisions:

- None; user decisions recorded in SOW-0103 apply.

## Implications And Decisions

1. 2026-06-11 routing, freeze, `.d.ts`, and versioning decisions recorded in
   SOW-0103 apply to this SOW: implementer
   `llm-netdata-cloud/minimax-m3-coder` (fallback `glm-5.1`), five
   `llm-netdata-cloud` reviewers, Rust/Go untouched, `.d.ts` with CI
   type-check, version aligned at next release.

## Plan

1. API-diff inventory and gate refresh.
2. Explorer port.
3. Netdata function port and wrapper.
4. Test/matrix integration, `.d.ts`, CI type-check.
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
  SOW-0104.

### 2026-06-12

- Activated 2026-06-12; activation committed as `bd776ac7`.
- Chunk 1 (Explorer port): the first minimax run died silently (0-byte
  output, no changes); the relaunch delivered `node/src/lib/explorer.js`
  (1781 lines) plus wiring and tests but ended mid-debug; the
  continuation run also died silently (second 0-byte minimax failure,
  recorded). The remaining defect was pinned by the project manager: a
  test assert-throws validator regexed `ALL_VALUES` against the actual
  `AllValues` message — typo-class, fixed directly along with four
  unused test imports. Verification by the project manager: full Node
  package suite green (conformance manifest + package tests); defaults
  spot-checked (limit 200, buckets 150, 8192-row control checks, 250ms
  progress); no directory-explore extension exists (parity-faithful
  from the start, unlike the Python chunk 1). Chunk 1 committed as
  `c2d2c11f`.
- Chunk 2 begins with the Python-proven sub-chunk split (2a foundation,
  2b request/discovery/merge/envelope, 2c stateful semantics); on a
  third silent minimax failure the chunk series moves to glm per the
  failure-handling clause.
- The third silent minimax failure occurred on the first chunk-2a
  launch (0-byte output, no files). ROUTING CHANGE RECORDED: the
  chunk-2 series implementer is now `llm-netdata-cloud/glm-5.1`;
  minimax is excluded from further implementation in this SOW and
  available as a reviewer for surfaces it did not implement (chunk 1
  Explorer remains minimax work; glm's netdata work gets the same
  conflict treatment in reviews as in SOW-0104, with the other four
  reviewers covering each implementer's surfaces independently).
- glm chunk 2a landed first-try (netdata foundation, 58/22 byte-exact,
  verified). Chunk 2b: first launch produced nothing (run stalled); the
  relaunch delivered request handling/discovery/merge/envelope (1689
  lines, verified green). Chunk 2c took four launches: two stall-kills
  (EXIT=124 at the 1800s guard, buffered output lost), one productive
  kill mid-iteration, then a solo success delivering the stateful
  semantics. Along the way the project manager pinned and fixed (typo-
  class) a property-vs-call bug in the pre-scan 304 path
  (`reader.header` is a property; the call threw into a broad catch
  making every file look stale -> wrongful 304), and identified the
  filtered-tail failure as MISSING page-window anchor machinery, which
  the final 2c run ported. Chunk 2 committed as `cb1d0899` (+3576).
- Infrastructure diagnosis recorded: the stall-kills cluster on the
  nova-hosted model slots (minimax 3x, glm 4x) including under an IDLE
  nova, while externally-backed pool members completed reliably all
  day. Chunk 3 first launches on glm stalled twice (full and compact
  prompts); ROUTING ROTATION RECORDED: chunk 3 implementer is
  `llm-netdata-cloud/deepseek-v4-pro`. deepseek landed chunk 3a
  first-try: wrapper CLI with 8 end-to-end tests (cancel-immediately
  -> 499 per the Rust wrapper), `--node`/`--node-interpreter` fourth
  peer in both comparator runners with defaults unchanged; verified
  green by the project manager.
- Review-conflict ledger: minimax implemented chunk 1 (Explorer); glm
  chunks 2a-2c; deepseek chunk 3; at review time each implementer's
  surfaces are covered independently by the four non-author reviewers.
- Chunk 3b launched on deepseek (d.ts + facade visitor + lint tidy +
  README).
- Chunk 3b landed on retry: 966-line `node/index.d.ts` with strict tsc
  typecheck script (verified green), facade `SdJournalVisitUniqueValues`
  with tests, lint tidy, README sections. Chunk 3 committed as
  `9583e5bb` (amended to include the visitor test files initially
  missed in staging — caught by project-manager status review).
- Validation phase (2026-06-12/13, all comparator runs by the project
  manager against the live journal read-only; ~half of implementer
  launches stalled at the 1800s guard and were retried, recorded):
  - Fix 1 (deepseek): header-derived source-summary bounds — the same
    chunk-scope gap Python had; info fixture green after.
  - Fix 2: wrapper stdout-drain (responses beyond one 8KiB pipe buffer
    were truncated by process.exit — found by the project manager from
    the run record's exact stdout_bytes=8192) plus window prefilter.
  - Fix 3: HEADER-ONLY per-file bounds reads — project-manager-unified
    root cause: FileReader.open in the no-mmap design reads whole
    files, so bounds for 7338 files meant 144GiB of I/O; info wall
    314s -> 0.4s after; data requests stopped burning their budgets
    into partial responses. Performance-contract item with truncation
    tests.
  - Fix 4: histogram bucket boundaries snapped to the origin grid
    (seven fixtures shared the one diff) and the compact 304 envelope.
  - Fix 5: exclude-own-field-filter facet semantics; its own grid test
    raced on Date.now() and was repaired to pinned values in fix 6.
  - Fix 6: zero-count selected-filter-value inclusion in facet options
    and histogram dimensions (`add_zero_count_selected_filter_values`
    peer) — found by the project manager extracting both sides' actual
    facet options (references emit the selected value at count 0).
  - One-shot validation chunk committed as `314dade1`.
- FINAL PARITY EVIDENCE: one-shot comparator gate 10/10 (node_vs_sdk
  and node_vs_plugin, live `/var/log/journal` read-only); stateful gate
  5/5 first try with full step counts (frozen fresh-data fixture);
  full Node package suite green incl. typecheck. Node needed 6
  comparator fix rounds versus Python's 14 — the inherited playbook
  halved convergence.
- Reviewer round 1: glm YES (treated the d.ts drift as non-blocking on
  a typecheck-passes argument — overruled by the SOW's own ".d.ts
  covers the public API" criterion), mimo NO, qwen NO, deepseek NO,
  kimi produced findings but no verdict (cutoff; also went off-script
  offering to implement — it changed nothing; retried in round 2).
  Project-manager-validated blockers, all fixed:
  - per-file ExplorerControl never passed to the explore call (qwen;
    dead deadline/cancel mid-file, masked by between-file checks);
  - capEffectiveDisplay 32-bit `>>>` truncation dropping capabilities
    32+ (qwen); BigInt rewrite;
  - fabricated NetdataJournalFunction/.d.ts surface + wrong matchedRow
    type (deepseek/mimo/qwen/kimi); rewritten to the real surface plus
    a mechanical conformance test (tsc cannot catch declaration-vs-JS
    drift) whose FIRST RUN caught WriterLock missing from the public
    exports — exported (project-manager mechanical fix);
  - anchor-outside-window reset-to-Auto missing (kimi; ported);
  - index.d.ts absent from the npm files array (kimi; the declared
    types would not have shipped);
  - README documented the same fabricated API (kimi; corrected);
  - 'binary' -> 'latin1' encoding minors (deepseek).
  Both parity gates reconfirmed post-fixes (10/10, 5/5); suite and
  typecheck green. Fixes committed; round 2 launches with identical
  scope plus fix notes.
- Follow-up noted for mapping: port the d.ts-style export-conformance
  test pattern to Python (its __init__ exports lack an equivalent
  mechanical check) — SOW-0106 or a hygiene SOW.
- Review rounds 2-4 (cumulative, never-narrow scope; each round found a
  strictly smaller class of issue, confirming convergence):
  - Round 2: qwen NO -> data-only `stopWhenRowsFull` never set (dead
    early-stop). Fixed + tested (`5e77d77f`). Investigating it, the
    project manager found two same-class gaps: per-file realtime slack
    not threaded (fixed) and the Rust `ExplorerSamplingState` budget
    engine unported in BOTH Node and Python (zero gate impact; no
    fixture exceeds the budget) -> tracked in pending SOW-0107.
  - Round 3: mimo/glm/qwen/deepseek YES; kimi NO -> `.d.ts` METHOD drift
    (`FileReader.match()` and `FilterBuilder.and/or/build` declared but
    absent; wrong `addMatch` signature) that the round-1 conformance
    test missed because it only checked class existence; plus PRIORITY
    facet options not sorted numerically (Rust `sort_facet_options`).
    Fixed (`cc33aa14`): d.ts corrected, conformance test extended to
    assert prototype methods, Node `_sortFacetOptions` ported with a
    value-pinning test; the Python facet-sort twin added to SOW-0107.
  - Round 4: deepseek YES; glm NO with four validated findings (it
    reversed its own round-3 YES, the value of independent deep review):
    (1) FTS parsed but never APPLIED on the Netdata production path -
    a correctness bug (queries with text search returned unfiltered
    results), symmetric with Python, hidden because the shared FTS
    fixture's Oct-2022 window is empty on the host; FIXED in Node with
    `_parseFtsQueryPatterns` + query threading + a synthetic triggering
    test proving Node now filters identically to Rust; Python twin
    tracked in SOW-0107. (2) FTS cache-hit row-drop - did not manifest
    once FTS was applied (the triggering test matches Rust exactly).
    (3) Index strategy O(N^2) collection + filter-validation - secondary
    public surface not on the Netdata path and not gate-exercised;
    tracked in SOW-0107 for dedicated Compare-mode validation.
    (4) d.ts drift the conformance test could not see (entire FileHeader
    interface camelCase vs snake_case runtime; phantom
    `FileReader.openBuffer`/`closed`, `DirectoryReader.closed`/`files`);
    FIXED and the conformance test extended to assert statics, forbidden
    phantom statics, and FileHeader fields against a real runtime header
    object. kimi recorded UNUSABLE as a reviewer: twice it broke the
    read-only role and tried to orchestrate (round 1 offered to
    implement; round 4 attempted to spawn its own reviewer batch); its
    round-3 findings were captured and fixed regardless. After the
    round-4 fixes (`0afd96dc`) both parity gates reconfirmed (one-shot
    10/10, stateful 5/5); a round-5 re-confirm of the usable reviewers
    on the final code follows.
  - Cross-cutting lesson (also in SOW-0107): the comparator gates
    validate only what their fixtures exercise. FTS (empty window),
    sampling (sub-budget rows), and the Index strategy (unfixtured) were
    all stubbed-or-partial in BOTH the Node and the already-closed
    Python ports and still passed. Triggering fixtures are now mandatory
    for threshold/conditional features; the Python twins are tracked in
    SOW-0107 for the user's decision on depth.


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
