# SOW-0105 - Node.js Explorer And Netdata Parity To Rust

## Status

Status: open

Sub-state: pending; activates after SOW-0104 closes (program order decided by
the user on 2026-06-11).

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

Status: blocked

Blocked on: SOW-0104 close (user-decided program order; one implementation SOW
at a time). At activation, the gate must be refreshed with the fresh API-diff
inventory results before implementation starts. All other gate content is
prepared:

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
