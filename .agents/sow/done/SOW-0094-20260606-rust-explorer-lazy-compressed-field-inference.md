# SOW-0094 - Rust Explorer Lazy Compressed Field Inference Experiment

## Status

Status: closed

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: closed without implementation on 2026-06-21 by user decision. The
experiment is not important enough to keep in the active backlog.

## Requirements

### Purpose

Evaluate whether the Rust Explorer can avoid unnecessary decompression of
irrelevant compressed DATA objects while preserving exact Netdata-shaped query
semantics and the journal format's row/field ordering contracts.

This SOW exists for performance completeness. The immediate priority remains
stabilizing and promoting the Explorer API; this optimization must not distract
from that work until the API boundary is stable.

2026-06-21 closeout:

- User decision: close this SOW; do not implement the experiment.
- Reason: the experiment is not important enough for the project backlog.
- Implication: default Rust Explorer behavior remains unchanged, and future
  compressed-DATA Explorer optimization would need a new user-approved SOW with
  fresh evidence.

### User Request

The user requested a pending SOW for an experimental optimization:

1. Skip decompressing compressed fields until all uncompressed fields in the
   row are examined and required facets are still not satisfied. This may not
   speed up every query, because not all rows have all facets, but the
   behavior is worth testing.
2. When decompression is required because facets are still unresolved, inspect
   `next_field_offset` first. If the next DATA object is already cached, that
   reveals the current compressed DATA object's field name through the
   same-field chain.
3. If facets are still not satisfied after the `next_field_offset` check,
   decompress the DATA payload to see the field and value.

### Assistant Understanding

Facts:

- Compressed DATA hides the complete `FIELD=VALUE` payload. The field name is
  not available without decompression unless it can be inferred from journal
  structure.
- DATA objects have `next_field_offset`; FIELD objects have
  `head_data_offset`.
- systemd links DATA objects into a same-field chain by writing the previous
  field-head DATA offset into the new DATA object's `next_field_offset`, then
  updating the FIELD head to the new DATA object.
- Current Rust Explorer traversal classifies row DATA offsets by loading the
  DATA payload and splitting `FIELD=VALUE`; for compressed DATA this
  decompresses the payload before field lookup.
- SOW-0083 measured full index-derived facet and histogram aggregation and did
  not make it the default, because it regressed many Netdata-shaped high-facet
  and filtered queries.
- SOW-0083 is the controlling evidence against switching this SOW to a
  FIELD-index-first design. It showed that index-derived aggregation depends
  heavily on facet count and filter selectivity, and that high-facet or
  selective-filter queries can be much slower than traversal.

Inferences:

- A lazy same-field-chain inference cache may avoid decompression for unique
  compressed fields such as `MESSAGE` when the same field's older DATA object
  has already been classified.
- This is not the same as full index-derived aggregation. The proposed
  optimization still uses the SOW-0082/SOW-0093 traversal model and only tries
  to avoid decompression inside that traversal.
- Forward scans are the most likely to benefit because
  `next_field_offset` points to an older DATA object that may already be
  classified. Backward scans may benefit less unless a bounded lookahead or
  chain-chase strategy is proven safe and fast.

Unknowns:

- Whether the extra header reads, row-local deferral, and offset lookup cost
  are lower than directly decompressing the skipped DATA objects.
- How often real files place required facets before or after compressed fields.
- Whether this optimization can preserve exact `FirstValue` semantics without
  adding enough tracking to erase its performance benefit.

### Acceptance Criteria

- Do not change the default Explorer behavior until benchmark evidence proves
  a strict improvement or a clear opt-in value.
- Preserve exact logical output for the accepted Explorer contracts:
  - filters and timeframe;
  - selected facets and counters;
  - selected histogram;
  - returned rows;
  - FTS when enabled;
  - `ExplorerFieldMode::FirstValue` and `ExplorerFieldMode::AllValues`.
- Implement the experiment behind an explicit experimental option or branch
  during measurement. The option must not be recommended for production until
  this SOW closes with evidence.
- Add counters that report at least:
  - compressed DATA objects seen;
  - compressed DATA objects deferred;
  - compressed DATA objects decompressed;
  - compressed DATA objects skipped through inferred field identity;
  - `next_field_offset` inference hits and misses;
  - row-level deferral fallbacks;
  - first-value ordering blockers.
- Benchmark the current traversal against the lazy inference experiment on:
  - synthetic files where compressed `MESSAGE` appears before required facets;
  - synthetic files where compressed `MESSAGE` appears after required facets;
  - synthetic files where some required facets are missing from many rows;
  - synthetic files with duplicate same-field values in a row;
  - FTS-disabled and FTS-enabled queries;
  - compressed required facet or histogram fields;
  - real-corpus queries from SOW-0093;
  - forward and backward scans.
- Record rows/s, wall time, CPU time, peak RSS, DATA refs seen, payloads loaded,
  payloads decompressed, inference hit/miss rate, and exact output equality
  status.
- Reject or leave opt-in only if the optimization is query-shape sensitive,
  regresses high-facet Netdata-style queries, breaks first-value semantics, or
  adds maintenance complexity without a meaningful measured win.
- Keep durable reports sanitized. Raw journal payloads and raw function output
  stay under `.local/`.

## Analysis

Sources checked:

- Rust Explorer traversal and DATA classification hot path.
- Rust `journal-core` DATA object header definition.
- systemd DATA/FIELD object layout and DATA-to-FIELD chain construction.
- SOW-0083 index-derived facet/histogram break-even evidence.

Current state:

- Current Explorer traversal calls `classify_data_for_accumulator()` for each
  DATA offset in a candidate row. The classification path increments payload
  load counters, checks whether the DATA object is compressed, reads the
  payload, then splits `FIELD=VALUE` before field lookup.
  Evidence:
  `rust/src/journal/src/explorer.rs:1075`
  `rust/src/journal/src/explorer.rs:1080`
  `rust/src/journal/src/explorer.rs:1596`
  `rust/src/journal/src/explorer.rs:1598`
  `rust/src/journal/src/explorer.rs:1599`
  `rust/src/journal/src/explorer.rs:1604`
  `rust/src/journal/src/explorer.rs:1621`
- Rust DATA object headers expose `next_field_offset`, but not a direct FIELD
  object back-pointer.
  Evidence:
  `rust/src/crates/journal-core/src/file/object.rs:575`
  `rust/src/crates/journal-core/src/file/object.rs:579`
- systemd's DATA object layout has `next_field_offset`; FIELD objects have
  `head_data_offset`.
  Evidence:
  `systemd/systemd @ 88b9acbc2b6a`
  `src/libsystemd/sd-journal/journal-def.h:77`
  `src/libsystemd/sd-journal/journal-def.h:81`
  `src/libsystemd/sd-journal/journal-def.h:101`
  `src/libsystemd/sd-journal/journal-def.h:105`
- systemd links new DATA objects into the FIELD chain by storing the previous
  `head_data_offset` in `data.next_field_offset`, then setting the FIELD head
  to the new DATA offset.
  Evidence:
  `systemd/systemd @ 88b9acbc2b6a`
  `src/libsystemd/sd-journal/journal-file.c:1911`
  `src/libsystemd/sd-journal/journal-file.c:1917`
  `src/libsystemd/sd-journal/journal-file.c:1918`
- systemd's unique field-value enumeration walks the FIELD object's
  `head_data_offset`, then follows each DATA object's `next_field_offset`.
  Evidence:
  `systemd/systemd @ 88b9acbc2b6a`
  `src/libsystemd/sd-journal/sd-journal.c:3382`
  `src/libsystemd/sd-journal/sd-journal.c:3388`
  `src/libsystemd/sd-journal/sd-journal.c:3394`
- SOW-0083 showed that full index-derived aggregation can win on narrow
  unfiltered queries, but regresses high-facet and selective-filter
  Netdata-style queries. This SOW must not reintroduce that rejected default
  strategy under another name.
  Evidence:
  `.agents/sow/done/SOW-0083-20260602-index-derived-facet-histogram-optimization.md:241`
  `.agents/sow/done/SOW-0083-20260602-index-derived-facet-histogram-optimization.md:252`
  `.agents/sow/done/SOW-0083-20260602-index-derived-facet-histogram-optimization.md:255`
  `.agents/sow/done/SOW-0083-20260602-index-derived-facet-histogram-optimization.md:260`
  `.agents/sow/done/SOW-0083-20260602-index-derived-facet-histogram-optimization.md:261`
  `.agents/sow/done/SOW-0083-20260602-index-derived-facet-histogram-optimization.md:263`
  `.agents/sow/done/SOW-0083-20260602-index-derived-facet-histogram-optimization.md:267`
  `.agents/sow/done/SOW-0083-20260602-index-derived-facet-histogram-optimization.md:270`
  `.agents/sow/done/SOW-0083-20260602-index-derived-facet-histogram-optimization.md:273`
- The concrete SOW-0083 measurements that block pushback toward
  FIELD-index-first as the default are:
  - generated compact, 20 facets: traversal `479,053` rows/s, index
    `476,242` rows/s, index/traversal `0.99x`;
  - NetFlow real, 20 facets: traversal `951,922` rows/s, index `430,160`
    rows/s, index/traversal `0.45x`;
  - generated compact, selective filter + 3 facets: traversal `668,486`
    rows/s, index `236,114` rows/s, index/traversal `0.35x`;
  - generated compact, highly selective filter + 20 facets: traversal `2,508`
    rows/s, index `3` rows/s, index/traversal `0.001x`;
  - NetFlow real, broad filter + 20 facets: traversal `1,186,358` rows/s,
    index `484,270` rows/s, index/traversal `0.41x`.

Risks:

- Correctness risk: deferring compressed DATA can change first-value semantics
  if an unresolved compressed DATA object physically appears before a later
  uncompressed DATA object with the same field name. The implementation must
  either prove exact first-value handling or restrict the optimization to query
  modes where exactness is unaffected.
- Performance risk: every compressed DATA skip attempt adds at least a header
  read and cache lookup. On files with cheap decompression, few compressed
  irrelevant fields, or many missing facets, the optimization may regress.
- Locality risk: chasing more than one `next_field_offset` can become
  random-access-heavy and repeat the SOW-0083 failure mode.
- FTS risk: full-text search still needs payload expansion, so this
  optimization may be disabled or sharply limited for FTS.
- Maintenance risk: adding another Explorer traversal path can make the API
  harder to reason about unless it is implemented as a small, measured
  extension to the existing traversal path.

## Pre-Implementation Gate

Status: closed without implementation

Problem / root-cause model:

- Current Explorer traversal can decompress compressed DATA before it knows
  whether the field is relevant to the selected facets, histogram, FTS, or
  returned rows. This can waste CPU on unique compressed fields, especially
  `MESSAGE`, when the query only needs low-cardinality facets and a histogram.
- The journal format does not store a direct field name in the DATA header, but
  the same-field `next_field_offset` chain may allow field identity inference
  from already-classified DATA objects.

Evidence reviewed:

- Current Rust Explorer hot path evidence listed in `## Analysis`.
- Rust DATA header evidence listed in `## Analysis`.
- systemd DATA/FIELD chain evidence listed in `## Analysis`.
- SOW-0083 evidence that full index aggregation is query-shape sensitive and
  should not become the default planner without proof.
- SOW-0083 benchmark evidence that FIELD/DATA posting-list aggregation can be
  catastrophic at high facet count or with selective filters. Any future
  reviewer or implementer proposing a FIELD-index-first replacement must first
  explain why those measured regressions no longer apply.
- User direction on 2026-06-06 to create this as an experimental SOW and first
  stabilize/promote the Explorer API.

Affected contracts and surfaces:

- Rust Explorer traversal implementation.
- Rust Explorer query options and diagnostic counters if the experiment is
  exposed.
- Netdata function-boundary API and benchmark reports if this improves
  SOW-0093 query shapes.
- Reader performance specs if the optimization is accepted.

Existing patterns to reuse:

- SOW-0082 optimized traversal path.
- SOW-0083 `ExplorerStrategy::Compare` style for evidence-first strategy
  comparison.
- SOW-0083 rejection rule: keep traversal as the default for high-facet and
  filtered Netdata-shaped queries unless new measurements prove otherwise.
- Existing `ExplorerStats` counters.
- Existing DATA offset classification cache, extended only if profiling proves
  the extension is cheaper than decompression.
- SOW-0093 strict content comparator and real-corpus request fixtures.

Risk and blast radius:

- Rust-only experimental scope at first.
- High correctness blast radius if first-value semantics, duplicate same-field
  handling, FTS, or returned-row expansion is changed.
- Medium performance blast radius because the Explorer is intended as a key
  SDK API for Netdata-shaped log queries.
- Low data-loss risk because this is reader-only.

Sensitive data handling plan:

- Do not commit raw journal payloads, raw plugin outputs, raw SDK outputs,
  customer-identifying values, personal data, IPs, private endpoints, or
  proprietary incident details.
- Store raw measurements and real-corpus outputs under `.local/`.
- Durable SOW/spec/docs updates may contain sanitized counts, rates, hashes,
  filenames only when already sanitized, and code paths.

Implementation plan:

1. Reconfirm the SOW-0093 Explorer baseline on the selected large real-corpus
   requests and on focused synthetic files.
2. Prototype a row-local deferred compressed-DATA pass:
   - process uncompressed DATA first;
   - defer compressed DATA while required facets/histogram identities may still
     be satisfied by uncompressed fields;
   - stop row traversal early only when exactness rules prove no deferred DATA
     can change the row result.
3. Add lazy field inference from `next_field_offset`:
   - inspect only the immediate `next_field_offset` first;
   - if that DATA offset is already cached with field identity, infer the
     current DATA object's field identity;
   - skip decompression only when inferred field identity proves the DATA is
     irrelevant for the current query;
   - decompress when field identity is unknown, required, or FTS needs payload
     bytes.
4. Decide whether bounded chain chasing is worth testing only after the
   immediate-next prototype has measured positive evidence.
5. Preserve exact first-value and duplicate same-field behavior:
   - track unresolved earlier compressed DATA as first-value blockers; or
   - restrict the optimization to all-values mode until exact first-value
     support is proven.
6. Compare outputs through the strict SOW-0093 comparator and focused synthetic
   assertions before accepting any benchmark win.
7. Adopt the optimization only if it is a measured strict improvement for
   important query shapes and no significant regression is found.

Validation plan:

- Rust unit tests for:
  - compressed irrelevant field before facet;
  - compressed irrelevant field after facet;
  - missing facet rows;
  - duplicate same-field values with first-value mode;
  - all-values mode;
  - compressed required facet/histogram field;
  - FTS-enabled query that forces payload expansion.
- Strict SOW-0093 content comparison for representative real-corpus requests.
- Benchmark current traversal versus experimental traversal with repeated cold
  and warm runs.
- Profile any positive and negative cases before adopting.
- `git diff --check`.
- `.agents/sow/audit.sh`.
- Whole-SOW reviewer pass after implementation if this SOW is activated.

Artifact impact plan:

- AGENTS.md: no expected update unless the optimization changes the global
  reader performance contract.
- Runtime project skills: update only if a reusable Explorer optimization
  workflow is accepted.
- Specs: update reader/Explorer performance specs only if behavior or public
  options change.
- End-user/operator docs: update Rust README/API docs only if a public option
  or diagnostic becomes supported.
- End-user/operator skills: no expected update.
- SOW lifecycle: keep open in pending until SOW-0093 Explorer stabilization is
  complete and the user explicitly activates this experiment.
- SOW-status.md: update with pending state and dependency.

Open-source reference evidence:

- `systemd/systemd @ 88b9acbc2b6a`
  - `src/libsystemd/sd-journal/journal-def.h:77`
  - `src/libsystemd/sd-journal/journal-def.h:81`
  - `src/libsystemd/sd-journal/journal-def.h:101`
  - `src/libsystemd/sd-journal/journal-def.h:105`
  - `src/libsystemd/sd-journal/journal-file.c:1911`
  - `src/libsystemd/sd-journal/journal-file.c:1917`
  - `src/libsystemd/sd-journal/journal-file.c:1918`
  - `src/libsystemd/sd-journal/sd-journal.c:3382`
  - `src/libsystemd/sd-journal/sd-journal.c:3388`
  - `src/libsystemd/sd-journal/sd-journal.c:3394`

Open decisions:

- None. The user closed this experiment on 2026-06-21.

## Implications And Decisions

1. 2026-06-06 optimization sequencing decision
   - Decision: create this SOW as a pending experiment, but do not implement it
     now.
   - Reason: the Explorer API needs stabilization and promotion first.
   - Implication: this work is tracked and will not be forgotten, but it will
     not consume the current critical path.

2. 2026-06-06 required algorithm shape
   - Decision: the experiment must test the user's three-step shape:
     defer compressed DATA until uncompressed DATA are examined, then try
     `next_field_offset` inference, then decompress only when still needed.
   - Implication: a full FIELD-chain/index precompute is not the default plan
     for this SOW because SOW-0083 already showed that style can regress
     Netdata-shaped queries.

3. 2026-06-06 correctness constraint
   - Decision: exact output remains mandatory. The optimization may not trade
     correctness for speed.
   - Implication: first-value ordering and duplicate same-field handling are
     explicit acceptance gates, not follow-up cleanup.

## Plan

1. Keep this SOW pending until SOW-0093 stabilizes the Explorer API and
   function-boundary behavior.
2. When activated, build the smallest Rust-only experiment that can measure
   row-local compressed-DATA deferral and `next_field_offset` inference.
3. Validate exactness before trusting benchmarks.
4. Keep, restrict, or reject the optimization based on measured evidence.
5. 2026-06-21 closeout: plan rejected by user decision; do not implement.

## Delegation Plan

Implementer:

- Local implementation only if the user activates this SOW. Do not run
  external implementers unless the user explicitly changes routing.

Reviewers:

- Reviewer pool after complete implementation and local validation: minimax,
  kimi, qwen, glm, and mimo.

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

- If benchmarks show no meaningful win, record the evidence and reject the
  optimization.
- If correctness cannot be preserved for first-value mode without large
  overhead, either keep the optimization all-values-only behind an explicit
  option or reject it.
- If audit or reviewer findings expose hidden behavior changes, keep the SOW
  open until the findings are fixed or the experiment is removed.
- 2026-06-21: user rejected the experiment before implementation, so no
  benchmark or reviewer failure handling is needed.

## Execution Log

### 2026-06-06

- Created pending SOW from the compressed-DATA Explorer optimization
  discussion.
- Recorded systemd DATA/FIELD chain evidence and the current Rust Explorer
  hot-path evidence.
- Recorded the user requirement to defer compressed DATA, inspect
  `next_field_offset`, and decompress only when still needed.

### 2026-06-21

- User decided to close this SOW because the experiment is not important enough
  to keep.
- No code, tests, specs, public docs, or runtime skills changed.

## Validation

Acceptance criteria evidence:

- Closed by user decision before implementation. No acceptance criteria were
  implemented, and no public behavior changed.

Tests or equivalent validation:

- Not required; this is a no-code backlog closeout.
- Closeout validation command: `.agents/sow/audit.sh` passed on 2026-06-21
  after the SOW move. Audit reported 6 pending SOWs, no current SOWs, 112 done
  SOWs, clean status/directory consistency, clean sensitive-data guardrail, and
  final verdict `SOW initialization complete and clean`.

Real-use evidence:

- Not applicable; this SOW was closed without implementation.

Reviewer findings:

- Not required; no implementation was performed.

Same-failure scan:

- No code was changed for this SOW.

Sensitive data gate:

- The SOW contains only sanitized source references, code paths, and aggregate
  design facts. No raw journal payloads or sensitive values were written.

Artifact maintenance gate:

- AGENTS.md: no update needed for SOW creation; project-wide performance rules
  already cover unnecessary decompression.
- Runtime project skills: no update needed for SOW creation; implementation
  workflow remains unchanged.
- Specs: no update yet; this is a pending experiment, not accepted behavior.
- End-user/operator docs: no update yet; no public API changed.
- End-user/operator skills: no update needed.
- SOW lifecycle: moved from `.agents/sow/pending/` to `.agents/sow/done/` with
  `Status: closed`.
- SOW-status.md: updated to remove this pending experiment and record the
  closeout.
- SOW status/directory consistency: verified by `.agents/sow/audit.sh` after the
  move.

Specs update:

- No spec update needed; no SDK behavior, API, data format, UX rule, business
  logic, operational guarantee, or known edge case changed.

Project skills update:

- No runtime project skill update needed; no reusable workflow changed.

End-user/operator docs update:

- No docs update needed. No public API or supported behavior changed.

End-user/operator skills update:

- No end-user/operator skill update needed.

Lessons:

- The journal DATA/FIELD same-field chain may expose field identity without
  decompression, but only after exactness and locality risks are measured.
- Not every theoretically valid optimization belongs in the backlog. If priority
  is low and risk/validation cost is high, closing the SOW is cleaner than
  carrying a stale experiment.

Follow-up mapping:

- None. Future work would require a new user-approved SOW with fresh evidence.

## Outcome

Closed without implementation. The user decided on 2026-06-21 that this
optimization experiment is not important enough to keep.

## Lessons Extracted

- Remove low-priority experimental SOWs instead of leaving them as stale backlog
  items.
- Keep the existing Rust Explorer behavior unchanged unless fresh evidence later
  justifies reopening the topic.

## Followup

None.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and later testing or use found broken behavior. Use a dated `## Regression - YYYY-MM-DD` heading at the end of the file. Never prepend regression content above the original SOW narrative.
