# SOW-0074 - Rust And Go Optimized Log Explorer Query API

## Status

Status: open

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: created from user requirement; awaiting activation after higher-priority active SOWs.

## Requirements

### Purpose

Build an SDK-native query API that helps callers implement high-performance log explorers without paying libsystemd-style `FOREACH_DATA` costs for fields that are irrelevant to filtering, faceting, FTS, or final display. The immediate fit-for-purpose target is Netdata-style journal exploration, where normal interactive queries should use journal indexes for filter slicing and avoid decompression, parsing, hashing, and value indexing for fields that do not need facet aggregation, FTS, or final display.

### User Request

Create a new SOW to build an optimized explorer/query API for Rust and Go. The SDK should provide this API itself, alongside existing reader APIs, so callers can build ideal/optimal log explorers.

The expected execution model is:

- Journal-native filters are index-backed exact set predicates:
  - positive: `FIELD IN [A, B, C]`, meaning `FIELD=A OR FIELD=B OR FIELD=C`;
  - negative: `FIELD NOT IN [A, B, C]`, meaning `FIELD!=A AND FIELD!=B AND FIELD!=C`.
- Positive and negative filters should slice candidate rows through journal DATA/entry indexes and must not require scan-time value expansion.
- FTS requires full field expansion.
- Display fields are not scan-time fields because the returned rows are normally limited.
- Facet fields are mandatory materialization fields during traversal of candidate rows.
- If the requested facet set is empty, or if every requested facet is already fully constrained by indexable filters, the API should use a no-aggregation fast path: slice by indexes, seek to the requested time boundary according to direction, enumerate matching entry offsets until the row limit is reached, and expand only returned rows.
- The API should expose filtered unique-value discovery: return all unique values of a target field for rows matching the same positive/negative filter model, without expanding unrelated fields.
- Expand selected facet fields during traversal, or all fields when FTS is requested.
- Decompress at display time unless the field is part of faceting or FTS.
- Expand all fields only for rows returned to the caller; ignored/skipped rows must not pay all-field expansion cost.

### Assistant Understanding

Facts:

- The existing libsystemd-compatible facade must remain available. This SOW adds an SDK-native optimized explorer/query API; it does not replace the facade.
- Journal ENTRY objects reference DATA object offsets, and DATA objects are reusable across entries.
- FIELD objects hold field names and link DATA objects belonging to that field.
- DATA payloads may be compressed as full `FIELD=VALUE` payloads; field names are not independently readable from compressed DATA payloads unless the reader uses FIELD object linkage or decompresses the DATA object.
- Netdata's current `systemd-journal.plugin` row path enumerates all fields, parses each `FIELD=VALUE`, and passes values into facets.

Inferences:

- A DATA-offset-aware API can avoid repeated `FIELD=VALUE` parsing for reusable DATA objects by caching field classification and materialized values by DATA object identity.
- Exact positive `FIELD IN [values]` filters can be resolved to DATA object offsets or posting lists once, then used as a per-field union of selected values without materializing filter values during row traversal. Collision verification during planning may inspect only relevant bucket candidates.
- Exact negative `FIELD NOT IN [values]` filters can be represented as an intersection of per-value complements, operationally subtracting the union of excluded posting lists from the current universe or from a previously narrowed candidate set. They still avoid value materialization, but cost depends on whether another positive/time constraint narrows the universe first.
- When no facet aggregation is required, exact-filter result enumeration can use DATA entry arrays/posting lists plus time-bound seeks to return the requested rows without scanning or materializing candidate row fields. This still enumerates matching ENTRY offsets and expands returned rows; it does not mean no entry movement at all.
- Filtered unique-value discovery can be implemented as an index operation: build the candidate row set from filters, then walk DATA objects for the requested target field and return/count only values whose posting list intersects the candidate set. This still costs at least `O(unique values for target field)` when the caller asks for every possible target value.
- Compressed DATA objects outside selected facet fields, FTS, and selected display rows can be skipped without decompression.
- There are no non-indexable fields in the journal-native filter model because the on-disk journal filter primitive is exact DATA payload membership. Regex, substring, numeric range on field values, or caller-defined predicates would be optional higher-level features outside the core optimized filter-slicing contract and would require a documented slow path.
- The most useful Rust/Go API will likely expose both a lower-level planning/visitor primitive and a higher-level explorer query helper.

Unknowns:

- The exact public API shape, naming, and type model need implementation-time design and user approval if multiple defensible public contracts remain after code analysis.
- The cache eviction policy and memory budget for DATA-offset/value caches need benchmarking against synthetic and real corpus workloads.
- Historical or damaged journals may have missing or inconsistent FIELD linkage; the implementation must determine and test the correct fallback behavior.

### Acceptance Criteria

- Rust and Go expose an SDK-native explorer/query API that accepts a query plan containing at least journal-native positive/negative filter predicates, facet fields, FTS mode/query, display expansion requirements, traversal direction, row limits, and time bounds where supported by existing readers.
- The API uses DATA object identity to avoid repeated processing of reusable `FIELD=VALUE` pairs.
- The API resolves positive and negative journal-native filters through DATA object/posting-list indexes and does not materialize filter values during row traversal.
- The API has a no-aggregation fast path for queries with no facets and for queries whose requested facets are exactly the indexable filter dimensions. This path must avoid candidate-row field traversal and only enumerate matching entry offsets plus returned-row display expansion.
- The API exposes filtered unique-value discovery for a target field under the same positive/negative filter plan. It must support optional value counts and pagination/limits where needed for high-cardinality fields.
- Filtered unique-value discovery must use journal DATA/FIELD indexes and posting-list intersections when available; it must not scan and expand every candidate row merely to discover values of the target field.
- The API can classify DATA references by field name without decompressing compressed DATA payloads when FIELD linkage is sufficient.
- Non-scope DATA objects are not decompressed or parsed during row traversal when FTS is not requested and the row is not selected for display expansion.
- Selected facet fields are materialized only for candidate rows that survive indexable filter slicing.
- FTS mode intentionally expands all fields and documents that it is the expensive mode.
- Returned/display rows can be fully expanded after row selection.
- Rust and Go produce equivalent query results for filters, facets, FTS, row counts, selected rows, timestamps, and display fields against the shared fixtures and generated corpora.
- Correctness is validated against the naive existing SDK reader path for all supported file variants: regular, compact, uncompressed DATA, zstd/xz/lz4 DATA compression, FSS where applicable, and mixed directories where the existing reader supports them.
- Performance benchmarks separately report:
  - low-level traversal with no value materialization;
  - positive filter-only exact matches through indexes with no scan-time value materialization;
  - negative filter-only exact matches through posting-list exclusion with no scan-time value materialization;
  - mixed positive and negative filters where positive values are OR within one field, negative values are AND-NOT within one field, and different fields are ANDed;
  - no-facet filtered top-N row retrieval through indexes with returned-row expansion only;
  - filter-equal-facet top-N row retrieval with counts derived without facet field materialization where the requested facet dimensions are fully constrained by the filters;
  - facet-only selected fields;
  - filter plus facets, where filters slice candidate rows and facets materialize only selected fields for candidates;
  - filtered unique-value discovery for low-cardinality and high-cardinality target fields, with and without positive/negative filters;
  - FTS all-fields mode;
  - selected-row display expansion;
  - compressed non-scope-field skip behavior.
- Benchmarks include Rust and Go, and include at least one workload where most fields are irrelevant to selected filters/facets.
- Instrumentation or tests prove skipped compressed non-scope fields are not decompressed.
- Existing libsystemd-compatible facade behavior and current reader APIs remain backward compatible.
- Public docs explain when to use the explorer API versus the facade and lower-level payload visitor.

## Analysis

Sources checked:

- `systemd/systemd @ cf3156842209`
  - `src/libsystemd/sd-journal/journal-def.h:62` - DATA object fields include DATA hash links, FIELD-chain link, entry links, and payload storage.
  - `src/libsystemd/sd-journal/journal-def.h:86` - FIELD object stores field name and head DATA offset.
  - `src/libsystemd/sd-journal/journal-def.h:98` - ENTRY object stores DATA object offsets.
  - `src/libsystemd/sd-journal/journal-file.c:1911` - writer creates FIELD objects by splitting DATA at `=`.
  - `src/libsystemd/sd-journal/sd-journal.c:682` - discrete matches find DATA objects and move through matching DATA entries.
  - `src/libsystemd/sd-journal/sd-journal.c:718` - AND matches repeatedly jump between indexed match streams until all match.
  - `src/libsystemd/sd-journal/journal-file.c:3240` - DATA entry arrays can be bisected for a requested location/time.
- `netdata/netdata @ 7e9cbb5dab6f`
  - `src/collectors/systemd-journal.plugin/systemd-journal-execute.h:37` - current plugin enumerates all row DATA.
  - `src/collectors/systemd-journal.plugin/systemd-journal-execute.h:80` - current plugin passes every parsed value to facets.
  - `src/libnetdata/facets/facets.c:591` - facet value indexing hashes the value bytes.
  - `src/libnetdata/facets/facets.c:1963` - FTS can copy and scan values.
  - `src/libnetdata/facets/facets.c:2052` - retained rows are materialized into row dictionaries.
- SDK repository:
  - `rust/src/journal/src/lib.rs:564` - current low-level Rust visitor returns payload bytes for current-entry DATA.
  - `rust/src/crates/journal-core/src/file/file.rs:916` - uncompressed DATA returns a borrowed payload slice; compressed DATA is decompressed before visitor callback.

Current state:

- Existing low-level visitors expose `FIELD=VALUE` payloads, not DATA object identities plus lazy value access.
- Existing facade and low-level payload APIs are optimized for libsystemd-like enumeration, not for explorer query planning.
- Existing public `query_unique` APIs expose only unfiltered field-wide unique values and do not accept a filter plan.
- The SOW-0064 reader experiments showed a large difference between low-level traversal/key classification and full payload hashing/scanning. The exact benchmark values are tracked in SOW-0064, not repeated here as final acceptance evidence.

Risks:

- Public API design risk: if the API is too Netdata-specific, it will not be a reusable SDK explorer API.
- Compatibility risk: relying only on FIELD object linkage may fail on historical or damaged journals; fallback rules need explicit tests.
- Performance risk: cache maintenance can be more expensive than direct parsing on low-cardinality or small files; benchmark-driven cache policy is mandatory.
- Memory risk: per-DATA-object caches can grow large on high-cardinality journals; callers need bounded or observable memory behavior.
- Correctness risk: exact-filter optimization by DATA offset/posting list must preserve journal semantics when repeated fields, positive `IN` uses OR within a field, negative `NOT IN` uses AND-NOT within a field, different fields are ANDed, binary values, compressed payloads, mixed files, or directory traversal are present.
- High-cardinality unique-value risk: asking for all values of a high-cardinality field is inherently proportional to the number of unique target-field DATA objects, even when filter slicing avoids row expansion. The API needs limits, pagination, and clear counters so callers do not accidentally build unbounded UI responses.
- Semantics risk: the no-aggregation fast path is valid only when no facet value distribution needs to be calculated beyond values fully constrained by indexable filters. If the caller requests any unconstrained facet, traversal/materialization of that facet for candidate rows remains required.
- Integration risk: Netdata still needs component integration SOWs after this SDK API exists.

## Pre-Implementation Gate

Status: needs-user-decision

Problem / root-cause model:

- The libsystemd-style API returns every current-row `FIELD=VALUE` payload. That shape forces callers to parse and often touch every value before they know whether a field matters for the query.
- For explorer workloads, row eligibility from positive and negative journal-native filters should be computed from journal indexes/posting lists, not by expanding filter field values during traversal. When no unconstrained facets are requested, the query does not need candidate-row field traversal at all; it only needs indexed candidate enumeration, time-bound positioning, row-limit collection, and returned-row display expansion. Facet aggregation requires selected facet field values only for candidate rows when at least one unconstrained facet is requested. Full row expansion is only needed for final returned rows, and FTS is the explicit all-fields expensive mode.
- The journal format already has reusable DATA objects and FIELD linkages that can support a lower-cost query plan, but the SDK does not yet expose an API organized around those identities.

Evidence reviewed:

- `systemd/systemd @ cf3156842209`
  - `src/libsystemd/sd-journal/journal-def.h:62`
  - `src/libsystemd/sd-journal/journal-def.h:86`
  - `src/libsystemd/sd-journal/journal-def.h:98`
  - `src/libsystemd/sd-journal/journal-file.c:1911`
  - `src/libsystemd/sd-journal/sd-journal.c:682`
  - `src/libsystemd/sd-journal/sd-journal.c:718`
  - `src/libsystemd/sd-journal/journal-file.c:3240`
- `netdata/netdata @ 7e9cbb5dab6f`
  - `src/collectors/systemd-journal.plugin/systemd-journal-execute.h:37`
  - `src/collectors/systemd-journal.plugin/systemd-journal-execute.h:80`
  - `src/libnetdata/facets/facets.c:591`
  - `src/libnetdata/facets/facets.c:1963`
  - `src/libnetdata/facets/facets.c:2052`
- SDK repository:
  - `rust/src/journal/src/lib.rs:564`
  - `rust/src/crates/journal-core/src/file/file.rs:916`

Affected contracts and surfaces:

- Rust public reader/query API.
- Go public reader/query API.
- Rust and Go internal journal object readers.
- Shared conformance fixtures and interoperability tests.
- Reader benchmark harness and standard benchmark reports.
- SDK docs explaining reader API tiers.
- Future Netdata integration SOWs, especially systemd-journal plugin and otel-signal-viewer reader integration.

Existing patterns to reuse:

- Existing Rust and Go low-level entry traversal and DATA payload visitors.
- Existing Rust and Go directory readers where applicable.
- Existing benchmark reporting from SOW-0059 and reader-core benchmark harnesses.
- Existing mixed-directory and compression interoperability matrices.
- Existing libsystemd-compatible facade as the correctness reference for full expansion.

Risk and blast radius:

- API additions should be additive. Existing facade, payload visitor, and high-level entry APIs must not regress.
- Internal reader changes can affect all Rust/Go reader paths; shared tests must run after implementation.
- Compression skip behavior must be proved with instrumentation because a passing result alone does not prove decompression was avoided.
- Caching must be bounded or configurable to avoid pathological memory growth on high-cardinality data.

Sensitive data handling plan:

- Tests and SOW evidence must use generated fixtures, sanitized corpus summaries, or hashes/counts. Durable artifacts must not include raw journal payloads from real systems, raw hostnames, customer identifiers, personal data, private endpoints, tokens, SNMP communities, or other sensitive values.

Implementation plan:

1. Analyze Rust and Go reader internals and design the public query-plan, filtered unique-values, and result API. Record any remaining public API decision for user approval before coding if multiple viable shapes remain.
2. Add a lower-level DATA-reference visitor in Rust and Go that can expose DATA identity, field name, compression status, and lazy value materialization while preserving row-scoped lifetime guarantees.
3. Build a planner/executor layer that separates positive/negative filter slicing, no-aggregation top-N retrieval, facet materialization, FTS expansion, and display expansion. Cache field classification and decoded facet/display values by DATA object identity only when needed.
4. Add correctness tests comparing optimized results to full naive expansion across regular, compact, compressed, and mixed fixtures.
5. Add decompression counters or equivalent instrumentation to prove non-scope compressed DATA objects are skipped.
6. Add benchmarks for Rust and Go across the accepted explorer workloads and update standard reports.
7. Update docs/specs with API tier guidance and performance caveats.

Validation plan:

- Rust package tests for affected reader/query crates.
- Go package tests for affected reader/query packages.
- Shared fixture tests comparing optimized and naive query outputs.
- No-aggregation fast-path tests for no facets and filter-equal-facet requests, proving candidate-row field traversal and compressed non-returned-row decompression do not occur.
- Filtered unique-values tests for positive filters, negative filters, mixed filters, binary values, repeated fields, high-cardinality values, empty results, limits/pagination, and counts.
- Compression skip tests with compressed irrelevant fields, selected compressed facet fields, and compressed filter fields resolved through indexes.
- Cross-language Rust-vs-Go query parity tests.
- Directory/mixed-directory reader matrix where supported by the new API.
- Reader benchmark reports for Rust and Go, including uncompressed and compressed skip workloads.
- Same-failure searches for unexpected decompression, repeated full-payload parsing, unbounded cache growth, and facade regressions.
- Whole-SOW read-only reviewer pass after implementation and local validation.

Artifact impact plan:

- AGENTS.md: likely unaffected unless the work creates a new durable workflow rule.
- Runtime project skills: update `project-journal-compatibility` if the new explorer API creates mandatory future validation patterns.
- Specs: update `.agents/sow/specs/` with the explorer/query API contract.
- End-user/operator docs: update Rust and Go README/API docs.
- End-user/operator skills: likely unaffected unless an output/reference skill is created for SDK consumers.
- SOW lifecycle: keep this SOW in pending until activated; complete and move to done with implementation commit when finished.
- SOW-status.md: update pending/current/completed state as the SOW advances.

Open-source reference evidence:

- `systemd/systemd @ cf3156842209`
  - `src/libsystemd/sd-journal/journal-def.h:62`
  - `src/libsystemd/sd-journal/journal-def.h:86`
  - `src/libsystemd/sd-journal/journal-def.h:98`
  - `src/libsystemd/sd-journal/journal-file.c:1911`
  - `src/libsystemd/sd-journal/sd-journal.c:682`
  - `src/libsystemd/sd-journal/sd-journal.c:718`
  - `src/libsystemd/sd-journal/journal-file.c:3240`
- `netdata/netdata @ 7e9cbb5dab6f`
  - `src/collectors/systemd-journal.plugin/systemd-journal-execute.h:37`
  - `src/collectors/systemd-journal.plugin/systemd-journal-execute.h:80`
  - `src/libnetdata/facets/facets.c:591`
  - `src/libnetdata/facets/facets.c:1963`
  - `src/libnetdata/facets/facets.c:2052`

Open decisions:

1. Public API shape after detailed implementation analysis.
   - Option A: expose only low-level DATA-reference visitor primitives.
     - Pros: smallest SDK surface, maximum caller control.
     - Cons: every caller must rebuild planner, cache, filter, and facet logic.
     - Risk: Netdata and other consumers duplicate complex logic.
   - Option B: expose both low-level primitives and a higher-level explorer query executor.
     - Pros: reusable optimized explorer behavior; lower integration burden; lower chance of consumers accidentally using slow patterns.
     - Cons: larger public API and more compatibility surface to maintain.
     - Risk: API must stay generic and not become Netdata-specific.
   - Recommendation: Option B.

2. Cache policy after benchmarks.
   - Option A: fixed internal cache with conservative memory cap.
     - Pros: simple default.
     - Cons: may underperform or overuse memory on some workloads.
   - Option B: bounded default plus caller-configurable memory/entry budget and counters.
     - Pros: production-friendly; lets Netdata tune high-performance deployments.
     - Cons: more options to document and test.
     - Risk: poor defaults can still hurt typical users.
   - Recommendation: Option B.

3. Historical fallback behavior.
   - Option A: require valid FIELD linkage for optimized classification and fail otherwise.
     - Pros: simpler and fastest.
     - Cons: less robust for historical or damaged files.
     - Risk: rejects files that naive readers can read.
   - Option B: use FIELD linkage when valid, fall back to payload split/materialization for affected DATA objects.
     - Pros: preserves compatibility; limits slow path to problematic objects.
     - Cons: more implementation complexity.
     - Risk: fallback can hide format issues unless counters expose it.
   - Recommendation: Option B.

## Implications And Decisions

1. Journal-native filter fields are not scan-time materialization fields.
   - User decision: journal-native filters are exact set predicates with two per-field strategies: positive `FIELD IN [A, B, C]` means `FIELD=A OR FIELD=B OR FIELD=C`; negative `FIELD NOT IN [A, B, C]` means `FIELD!=A AND FIELD!=B AND FIELD!=C`. Both must use journal DATA/entry indexes to slice candidate rows and must not be expanded during row traversal.
   - Implication: the optimized explorer API should make positive and negative exact-set predicates first-class. Regex, substring, numeric range on field values, or caller-defined predicates are outside this core optimized journal-native filter contract unless a later decision adds an explicit slow path.
   - Risk: negative-only filters over a wide time range may still require enumerating a large time/source universe to subtract excluded posting lists, even though they do not require field value materialization.

2. No-aggregation fast path for no facets or filter-equal-facet requests.
   - User decision: if the facet list is empty, or if requested facets are exactly the dimensions already fully constrained by indexable filters, the API should avoid traversal/materialization of candidate row fields. It should slice by filter indexes, seek to the time boundary for the requested direction, enumerate matching entries up to the row limit, and expand only returned rows.
   - Implication: the query planner needs to classify whether a request has unconstrained facet dimensions. If it does not, the executor can bypass facet traversal entirely.
   - Risk: if future UI semantics require distribution counts for values outside the selected filter values, this fast path would not be valid for that request and must fall back to facet aggregation.

3. Filtered unique-values API.
   - User decision: the optimized query API should support "unique values of target field under filters", for example `unique(FIELD1) WHERE FIELD2 IN [A, B, C] AND FIELD3 NOT IN [D, E, F]`.
   - Implication: this is not the same as the current libsystemd-style unfiltered `query_unique(field)` APIs. It needs the same filter planner as normal explorer queries and should use target-field DATA chains plus posting-list intersection instead of expanding all candidate rows.
   - Risk: high-cardinality target fields remain inherently expensive to enumerate completely. The public API should include limits, pagination/cursors, optional counts, and metrics/counters so callers can control cost.

Initial unresolved API-shape recommendations are recorded in the pre-implementation gate.

## Plan

1. Public API design and contract proof.
   - Scope: Rust and Go query-plan types, result shape, data-reference visitor shape, cache options, and documented semantics.
   - Risk: public API churn if implemented before the contract is clear.
   - Dependencies: current reader APIs and SOW-0064/SOW-0009 benchmark evidence.
2. Rust implementation.
   - Scope: DATA-reference classification, lazy materialization, query executor, counters, tests, benchmarks.
   - Risk: row lifetime and mmap/decompression buffer guarantees must remain sound.
   - Dependencies: existing Rust reader hot paths and facade tests.
3. Go implementation.
   - Scope: API parity with Rust, mmap-backed classification, lazy materialization, query executor, counters, tests, benchmarks.
   - Risk: avoiding extra allocations while keeping API safe and idiomatic.
   - Dependencies: existing Go reader hot paths and parity tests.
4. Cross-language validation and reporting.
   - Scope: fixture parity, mixed/compressed skip tests, benchmark reports, docs/spec updates.
   - Risk: benchmark noise; standard report format must be used.

## Delegation Plan

Implementer:

- Current project routing is local implementation by the project manager unless the user explicitly changes routing. This SOW should be implemented locally when activated.

Reviewers:

- Run whole-SOW read-only reviews after implementation and local validation using:
  - `llm-netdata-cloud/minimax-m2.7-coder`
  - `llm-netdata-cloud/kimi-k2.6`
  - `llm-netdata-cloud/qwen3.6-plus`
  - `llm-netdata-cloud/glm-5.1`
  - `llm-netdata-cloud/mimo-v2.5-pro`

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

- If public API design exposes unresolved product choices, stop and present numbered options to the user before implementation.
- If optimized results diverge from naive expansion, keep the SOW in progress and record exact fixture/query discrepancies.
- If performance is not materially better than existing traversal for non-FTS filtered/faceted workloads, profile before closing and either fix the issue or record a follow-up SOW with evidence.
- If reviewers do not vote production-grade, fix or disposition findings and rerun the same whole-SOW review scope.

## Execution Log

### 2026-05-31

- Created SOW from user requirement for a Rust and Go SDK-native optimized log explorer/query API.
- Recorded initial format evidence from systemd and Netdata plugin/facets evidence.
- Recorded filtered unique-values as an explicit required API: unfiltered `query_unique(field)` exists today, but filtered unique discovery under journal-native positive/negative filters does not.

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

- This SOW records source-code evidence and generated-design requirements only. It does not include raw journal payloads, secrets, credentials, bearer tokens, SNMP communities, community member names, customer names, personal data, non-private customer-identifying IPs, private endpoints, or proprietary incident details.

Artifact maintenance gate:

- AGENTS.md: no change at SOW creation time; revisit after implementation if workflow guardrails change.
- Runtime project skills: no change at SOW creation time; update after implementation if a new mandatory compatibility workflow emerges.
- Specs: pending implementation; explorer/query API contract should be added to product specs.
- End-user/operator docs: pending implementation; Rust and Go docs should be updated.
- End-user/operator skills: no current output/reference skill impact.
- SOW lifecycle: created in `.agents/sow/pending/` with `Status: open`.
- SOW-status.md: updated at SOW creation.

Specs update:

- Pending implementation.

Project skills update:

- Pending implementation decision.

End-user/operator docs update:

- Pending implementation.

End-user/operator skills update:

- No current output/reference skill impact.

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

Append regression entries here only after this SOW was completed or closed and later testing or use found broken behavior. Use a dated `## Regression - YYYY-MM-DD` heading at the end of the file. Never prepend regression content above the original SOW narrative.
