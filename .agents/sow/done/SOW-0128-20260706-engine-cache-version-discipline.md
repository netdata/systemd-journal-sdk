# SOW-0128 - Engine Index Cache Version Discipline

## Status

Status: completed

Sub-state: created 2026-07-06 from gaps surfaced by the Netdata
vendored-journal elimination analysis; awaiting decisions on retroactive
bump and consumer-visible namespace control. Completed 2026-07-06 by local
project-manager implementation per user routing decision.

## Requirements

### Purpose

Guarantee that semantically different `FileIndex` cache entries can never be
silently reused across SDK versions: bump `CACHE_VERSION` whenever index
semantics change, record that as a hard release rule, and give consumers a
supported cache-namespace control for their own migrations.

### User Request

The user directed (2026-07-06, during the Netdata vendored-journal
elimination work): SDK functionality gaps found by consumer integration are
filled in the SDK via SDK SOWs.

### Assistant Understanding

Facts:

- `journal-engine` `CACHE_VERSION: u32 = 2` is identical before and after
  commit `84fb1ed` ("Implement field name policy layers"), which changed
  index semantics: ND_REMAPPING bookkeeping-entry exclusion was removed from
  the indexer, so entry lists, histograms, and facet counts differ for
  journals containing such entries (`rust/src/crates/journal-engine/src/
  cache.rs:14`; pre-fork comparison: `netdata/netdata @ 17a7eb31da`
  `src/crates/journal-engine/src/cache.rs:17` with the exclusion logic in
  `src/crates/journal-index/src/file_indexer.rs:106-205`).
- Serialization stayed bit-compatible across the change (`FileIndex` struct
  byte-identical; roaring portable serialization; foyer 0.20.0 both sides) —
  so old entries deserialize cleanly and are REUSED, which is exactly the
  hazard: same version number, different meaning.
- `CACHE_VERSION` is part of `FileIndexKey`; a bump is a clean cache-miss +
  rebuild path (`journal-engine/src/indexing.rs` version handling). The
  mechanism works; the discipline of bumping it was not applied.
- Consumers cannot bump the SDK-owned constant. The Netdata migration works
  around this by changing the viewer's cache DIRECTORY default
  (netdata-side decision, 2026-07-06).
- Archived-file cache entries are treated as always-fresh and reused
  indefinitely (`journal-index/src/file_index.rs:94-103`), so stale
  semantics never age out on their own for archived journals.

Inferences:

- Any consumer that kept its cache directory across the vendored→0.7.x
  transition (or across `84fb1ed` if they consumed pre-0.3.0 builds) can
  serve mixed-semantics histograms/facets today without any error signal.

Unknowns:

- Whether any non-Netdata consumer population exists that a retroactive
  bump would churn (one-time full re-index cost).

### Acceptance Criteria

- A recorded release rule (spec + release checklist): any change to what the
  indexer includes/excludes/computes REQUIRES a `CACHE_VERSION` bump in the
  same change.
- Decision recorded and executed on a retroactive bump (2 → 3) to invalidate
  any pre-`84fb1ed`-semantics entries still in the wild.
- `FileIndexCacheBuilder` exposes a documented consumer-visible namespace
  (or version-salt) option so consumers can force clean rebuilds for their
  own semantic migrations without touching the SDK constant.
- Tests: a cache entry written under version N is not served under N+1;
  namespace-salted keys do not collide.

## Analysis

Sources checked:

- `rust/src/crates/journal-engine/src/cache.rs`, `src/indexing.rs`;
  `rust/src/crates/journal-index/src/file_index.rs`.
- SDK history around `84fb1ed`.
- `netdata/netdata @ 17a7eb31da` vendored `journal-engine`/`journal-index`
  (pre-change semantics).

Current state:

- Version constant unchanged across a semantic change; no consumer-side
  namespace control; archived entries reused forever.

Risks:

- Retroactive bump forces a one-time full re-index for all consumers
  (bounded, self-healing); NOT bumping leaves silent wrong counts for mixed
  caches indefinitely.

## Pre-Implementation Gate

Status: ready (user authorized local project-manager implementation on 2026-07-06)

Problem / root-cause model:

- The cache key encodes a version, but no process rule ties semantic
  indexer changes to bumping it; `84fb1ed` changed semantics without a
  bump, making version 2 ambiguous.

Evidence reviewed:

- See facts (file:line and commits above).

Affected contracts and surfaces:

- `CACHE_VERSION` constant, `FileIndexCacheBuilder` public API (additive
  option), release checklist/spec.

Existing patterns to reuse:

- Existing `FileIndexKey` version-keyed miss path (`indexing.rs`).

Risk and blast radius:

- Low code risk; one-time re-index cost on bump.

Sensitive data handling plan:

- Synthetic fixtures only.

Implementation plan:

1. User decision: retroactive bump 2→3 yes/no.
2. Add the release rule to specs/checklist.
3. Add the namespace option + tests.

Validation plan:

- Unit tests (version miss, namespace salt); spec/checklist review.

Artifact impact plan:

- Specs + release checklist updated; SOW-status ledgers updated.

Open-source reference evidence:

- `netdata/netdata @ 17a7eb31da`
  `src/crates/journal-engine/src/cache.rs:17`
  `src/crates/journal-index/src/file_indexer.rs:106-205`

Open decisions:

- 2026-07-06 user routing/design decision: the project manager implements and
  orchestrates this SOW directly; no separate external implementer model is
  used. Existing SOW analysis is planning evidence and hints, not a frozen
  design.
- Retroactive bump: bump `CACHE_VERSION` from 2 to 3. This is the
  long-term-best option because the cost is a bounded one-time reindex while
  the alternative permits silent stale semantics indefinitely.
- Namespace option shape: add a documented consumer namespace string folded
  into `FileIndexKey`, so consumers can force a clean rebuild without changing
  cache directories or SDK-owned constants.

## Implications And Decisions

- 2026-07-06: created per user direction that SDK gaps found during Netdata
  integration are filled in the SDK.

## Plan

1. Resolve decisions.
2. Implement rule + bump + namespace option.
3. Validate and review.

## Delegation Plan

Implementer:

- Local project-manager implementation per user routing decision on
  2026-07-06. No separate external implementer model is used for this SOW.

Reviewers:

- Read-only reviewers from the approved pool.

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

- Record blockers and missing evidence before changing scope.

## Execution Log

### 2026-07-06

- Created from the Netdata vendored-journal elimination deep analysis
  (netdata-side SOW: eliminate-vendored-journal-crates).
- Bumped Rust `journal-engine` `CACHE_VERSION` from 2 to 3.
- Added a cache namespace field to `FileIndexKey` and
  `FileIndexCacheBuilder::with_cache_namespace()`.
- Added tests proving version mismatch and namespace differences produce
  distinct cache keys.
- Second reviewer pass confirmed the cache behavior but found the process gap:
  future semantic cache/index changes were not recorded in runtime project
  skills. `.agents/skills/project-journal-compatibility/SKILL.md` now requires
  an explicit `CACHE_VERSION` decision for index semantics, cache schema,
  serialized index content, `FileIndexKey`, or consumer partitioning changes.
- Read-only Netdata impact check at `netdata/netdata @ 93d4f98c65b4` found
  `otel-signal-viewer-plugin/src/catalog.rs` builds a `FileIndexCacheBuilder`
  and also creates direct `FileIndexKey::new(...)` values; no forced Netdata
  source change is required, but optional consumer namespace adoption must use
  `FileIndexCacheBuilder::file_index_key()` or
  `FileIndexKey::new_with_namespace(...)` for those direct keys.

## Validation

Acceptance criteria evidence:

- `rust/src/crates/journal-engine/src/cache.rs` records `CACHE_VERSION = 3`
  and includes namespace in `FileIndexKey`.
- `rust/src/crates/journal-engine/src/indexing.rs` exposes builder-level
  namespace control and key construction.
- `.agents/sow/specs/product-scope.md` records the cache-version and namespace
  contract for future semantic index changes.

Tests or equivalent validation:

- `cargo test --manifest-path rust/Cargo.toml --workspace` - passed.
- `go test ./...` from `go/` - passed.
- `python3 tests/docs/check_wiki_docs.py` - passed.
- `python3 tests/docs/verify_examples.py` - passed 31/31 examples.
- Focused repair coverage passed through the full Rust workspace:
  `cache_namespace_keys_do_not_collide` now proves empty namespace remains
  equivalent to `FileIndexKey::new(...)`, and builder tests prove explicit
  consumer namespaces affect builder-created keys. The
  `old_serialized_cache_key_without_namespace_decodes_as_old_key` test proves
  pre-namespace serialized cache keys still decode through the empty namespace.

Real-use evidence:

- Unit tests exercise cache-key schema separation directly; no production cache
  files were read or modified.

Reviewer findings:

- 2026-07-06: read-only reviewers were run with the SOW filename and complete
  changed surface. Reviewers found one cache-error handling risk and one
  documentation footgun. Both were handled: cache lookup errors are now treated
  as misses and recomputed instead of dropping the file from results, and the
  builder namespace warning now states that it affects only keys created with
  `FileIndexCacheBuilder::file_index_key()` or
  `FileIndexKey::new_with_namespace()`, not keys created elsewhere with
  `FileIndexKey::new()`.
- 2026-07-06 repeat review: Claude voted not production-grade as closed SOWs
  because the cache-version discipline rule had not been captured in project
  skills. GLM timed out before a final verdict after partially reproducing real
  concerns. Minimax, Deepseek, Kimi, and Qwen returned production-grade or
  production-grade with non-blocking notes. The skill rule and empty-namespace
  equality test close the actionable cache findings.
- 2026-07-06 second repeat review: Claude failed with an API connection reset;
  GLM and Minimax timed out without final verdicts; Deepseek, Kimi, and Qwen
  returned production-grade with non-blocking notes. Deepseek noted that old
  serialized cache-key payloads without `namespace` should be explicitly tested;
  `#[serde(default)]` plus the old-key decode test now covers that compatibility
  path.

Same-failure scan:

- `journal-engine` cache key construction paths were updated together:
  direct `FileIndexKey` construction remains backward-compatible via the empty
  namespace, builder construction can opt into a consumer namespace, and cache
  lookup errors now take the same recompute path as cache misses.

Sensitive data gate:

- No sensitive data was used or recorded.

Artifact maintenance gate:

- `AGENTS.md`: no project-wide workflow change required.
- Runtime project skills:
  `.agents/skills/project-journal-compatibility/SKILL.md` updated so future
  index/cache semantic changes must decide and record `CACHE_VERSION` handling.
- Specs: product scope updated.
- End-user/operator docs: no consumer wiki change needed; this is an internal
  engine cache contract.
- SOW lifecycle: completed and moved to done.
- `.agents/sow/SOW-status.md`: updated.

Specs update:

- `.agents/sow/specs/product-scope.md` records cache version and consumer
  namespace rules.

Project skills update:

- `.agents/skills/project-journal-compatibility/SKILL.md` now records the
  cache-version discipline that reviewers identified as reusable workflow.

End-user/operator docs update:

- No consumer wiki page is dedicated to `journal-engine` cache keys; the product
  spec and API doc comments record the namespace/key-construction requirement.

End-user/operator skills update:

- No output/operator skills are maintained for this SDK surface.

Lessons:

- Cache schema and semantic changes need an explicit release rule, not just a
  local test. The spec now records that rule.

Follow-up mapping:

- No implementation follow-up remains for this SOW. The one-time cache reindex
  impact is expected after `CACHE_VERSION = 3`; no Netdata source change is
  required unless Netdata wants a consumer namespace distinct from the SDK
  default cache namespace.

## Outcome

Completed. Rust `journal-engine` cache keys now separate SDK cache-version
changes and consumer namespaces.

## Lessons Extracted

Versioned caches need both SDK-owned invalidation and consumer-owned namespace
control to avoid silent stale reuse.

## Followup

None.

## Regression Log

None yet.
