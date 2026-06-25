# SOW-0125 - Netdata Filter Operators Gap Analysis

## Status

Status: open

Sub-state: parked requirements and analysis record only; do not implement until the user explicitly resumes this SOW.

## Requirements

### Purpose

Preserve the full operator-support requirements and the analysis so far, so the project can later continue the design discussion without losing context or prematurely committing to implementation.

### User Request

Create an SOW for supporting additional Netdata row-filter operators, but do not execute it now. The user asked whether the SDK and Netdata function could hypothetically support the listed string and numeric operators, what they would mean, and what gaps exist today.

Requested string-field operators:

- `is`
- `is not`
- `is one of`
- `is not one of`
- `exists`
- `does not exist`
- FTS with Netdata simple patterns, including glob-like positive and negative expressions
- FTS with regex

Requested numeric-field operators:

- `is`
- `is not`
- `is one of`
- `is not one of`
- `is greater or equal than`
- `is less than`
- `is between`
- `is not between`
- `exists`
- `does not exist`

Additional constraints from the discussion:

- Journal payload values are strings/bytes, so numeric matching requires parsing string values and can be slow if implemented as row scans.
- Field types are not known upfront.
- Current Netdata payloads do not support negative matches for structured selections, so the request/selection payload must be extended.
- The current question is hypothetical capability and semantics, not approval to implement.

### Assistant Understanding

Facts:

- Current Rust and Go `ExplorerFilter` support only a field plus positive exact values.
- Current Netdata request parsing maps `selections` into exact positive filters and `query` into FTS terms.
- Current simple FTS already supports negative terms in the Explorer row-level FTS path.
- Regex FTS is not currently represented in the Netdata or Explorer filter contracts.
- Rust has a lower-level `journal-index` filter enum with an existence primitive, but it is not the Netdata Explorer contract.

Inferences:

- All requested operators are technically supportable.
- Efficient support requires a typed predicate AST, a planner, and set algebra over journal-native candidate sets rather than row expansion.
- Numeric comparisons can be made practical by enumerating unique DATA values for a field, parsing each unique value once, and unioning the matching DATA entry arrays.
- Negative predicates should generally be implemented as set difference from a positive query universe.
- Regex over field values can be handled similarly to numeric comparisons when scoped to a field; whole-row regex FTS is a different and more expensive class.

Unknowns:

- The final Netdata request schema for structured operators.
- Whether numeric parse failures should behave as non-match, special invalid bucket, or user-visible error.
- Whether multi-value fields should use `any` semantics, `all` semantics, or operator-specific semantics.
- Whether unknown field types should be inferred per query, provided by UI metadata, or left as caller-selected operator mode.

### Acceptance Criteria

- This SOW preserves the requirements, current support matrix, semantic questions, and recommended implementation architecture.
- No SDK source, tests, docs, specs, or public API are changed by this parked SOW.
- When resumed, implementation must not start until the open semantic decisions are resolved and recorded.

## Analysis

Sources checked:

- `rust/src/journal/src/explorer.rs`
- `go/journal/explorer.go`
- `rust/src/journal/src/netdata.rs`
- `go/journal/netdata.go`
- `rust/src/crates/journal-index/src/filter.rs`
- `.agents/sow/specs/systemd-journal-plugin-facets.md`

Current state:

- Rust `ExplorerFilter` is only `{ field, values }`: `rust/src/journal/src/explorer.rs:58`.
- Go `ExplorerFilter` is only `{ Field, Values }`: `go/journal/explorer.go:73`.
- Rust Netdata request parsing builds `filters = parse_filters(object.get("selections"))`: `rust/src/journal/src/netdata.rs:1448`.
- Go Netdata request parsing builds `filters := parseNetdataFilters(object["selections"])`: `go/journal/netdata.go:471`.
- Rust indexed filtering configures positive exact filters only: `rust/src/journal/src/explorer.rs:1528`.
- Go indexed filtering intersects positive exact filter result sets: `go/journal/explorer.go:1033`.
- Rust FTS row rejection recognizes negative FTS matches: `rust/src/journal/src/explorer.rs:3213`.
- Go FTS row rejection mirrors this behavior: `go/journal/explorer.go:2504`.

Current support matrix:

| Operator class | Current support | Notes |
| --- | --- | --- |
| string `is` | Supported | Exact positive field/value selection. |
| string `is one of` | Supported | Multiple exact values for one field. |
| string `is not` | Not supported structurally | Requires negative predicate/set difference. |
| string `is not one of` | Not supported structurally | Requires negative predicate/set difference. |
| string `exists` | Not supported in Netdata Explorer | Lower-level Rust index has existence concept, but not this surface. |
| string `does not exist` | Not supported | Requires field-existence universe and difference. |
| FTS simple patterns with negatives | Partially supported | Current FTS path supports positive and negative simple terms. |
| FTS regex | Not supported | Requires schema/API and implementation policy. |
| numeric `is` | Supported only as exact string bytes | No numeric typing. |
| numeric `is not` | Not supported | Requires parse semantics and negative set difference. |
| numeric `is one of` | Supported only as exact string bytes | No numeric normalization. |
| numeric `is not one of` | Not supported | Requires parse semantics and negative set difference. |
| numeric `>=` | Not supported | Requires parsing unique field values or row values. |
| numeric `<` | Not supported | Requires parsing unique field values or row values. |
| numeric `between` | Not supported | Requires parsing and range comparison. |
| numeric `not between` | Not supported | Requires parsing and negative set difference. |
| numeric `exists` | Not supported in Netdata Explorer | Same existence primitive needed as strings. |
| numeric `does not exist` | Not supported | Requires absence semantics. |

Risks:

- Performance: row-scanning numeric filters over large journals would violate the project performance contract.
- Semantics: missing fields, repeated fields, parse failures, empty strings, binary values, and multiple values per field need explicit rules.
- API compatibility: Netdata request payloads need additive structured operator support without breaking existing `selections`.
- Security: regex needs bounded compilation/evaluation rules to avoid pathological CPU use.
- UX consistency: field type inference can make the same field behave differently across files if not carefully defined.

## Pre-Implementation Gate

Status: blocked

Problem / root-cause model:

- The current Netdata filter model is exact-positive only for structured selections.
- The requested operator set needs negative predicates, existence predicates, numeric parsing/ranges, and regex predicates.
- Journal files store field payloads as bytes/strings, so numeric operators are higher-level query semantics, not native journal field types.

Evidence reviewed:

- Current Rust/Go Explorer filter structs and Netdata parser paths listed above.
- Current Rust/Go FTS negative-term behavior listed above.
- No external open-source implementation was checked because this SOW is a parked discussion artifact; implementation research must check current Netdata UI/API expectations and comparable log-query systems before code starts.

Affected contracts and surfaces:

- Netdata function request schema.
- Rust Explorer filter API.
- Go Explorer filter API.
- Facet-count semantics when active filters are negative or existence predicates.
- Candidate-set planner and performance contract.
- End-user/operator docs for supported filter syntax.

Existing patterns to reuse:

- Journal-native FIELD to DATA chains for enumerating unique field values.
- DATA entry arrays for candidate row sets.
- Existing exact-filter intersection path.
- Existing FTS negative-term representation and row rejection.
- Existing Rust `journal-index` filter concepts where they fit the Netdata Explorer surface.

Risk and blast radius:

- High design risk if semantics are not agreed before implementation.
- Medium API risk because Netdata payload shape must be extended.
- High performance risk for naive numeric and regex row scans.
- Medium test burden because Rust and Go must match, including edge cases.

Sensitive data handling plan:

- Use synthetic journals and sanitized fixtures only.
- Do not store real log lines, customer identifiers, personal data, private endpoints, bearer tokens, SNMP communities, or raw incident details in SOWs, specs, tests, or docs.

Implementation plan:

1. Do not implement while this SOW is parked.
2. When resumed, define structured predicate schema with backward-compatible mapping from current `selections`.
3. Define exact semantics for missing fields, repeated fields, numeric parse failures, binary values, case sensitivity, regex limits, and FTS scope.
4. Implement a Rust and Go predicate AST.
5. Implement a planner that uses journal-native FIELD/DATA chains and DATA entry arrays to build candidate sets for exact, one-of, existence, numeric range, and field-scoped regex predicates.
6. Implement negative predicates as candidate-set difference.
7. Keep whole-row FTS separate from field-scoped predicates unless the user approves a broader FTS redesign.
8. Add cross-language tests and Netdata request/response compatibility fixtures.

Validation plan:

- No validation is required for SOW creation beyond SOW audit.
- When resumed: shared Rust/Go fixtures for every operator, missing fields, repeated fields, parse failures, binary values, negative predicates, and multi-file query behavior.
- Benchmark numeric/range/existence filters against row-scan baseline and record performance evidence.
- Reviewer gate after a complete implementation chunk.

Artifact impact plan:

- AGENTS.md: likely unaffected.
- Runtime project skills: likely unaffected unless implementation creates a recurring operator-validation workflow.
- Specs: required when behavior is implemented.
- End-user/operator docs: required when behavior is implemented.
- End-user/operator skills: likely unaffected unless docs/spec output skills are added later.
- SOW lifecycle: pending/open parked SOW; not executable until user resumes it.
- SOW-status.md: update to list this parked pending SOW.

Open-source reference evidence:

- None checked for this parked SOW. Future implementation must inspect comparable log query/filter systems and record upstream repository identity plus commit if local mirrored repositories are used.

Open decisions:

1. Request schema for structured operators.
2. Missing-field semantics for negative operators.
3. Multi-value field semantics.
4. Numeric parse failure semantics.
5. Regex scope, engine, flags, and resource limits.
6. Whether field type inference belongs in SDK, UI, or caller-provided query metadata.
7. Whether FTS regex is row-wide, field-scoped, or both.

## Implications And Decisions

1. User decision recorded 2026-06-25: this SOW is not to be executed now.
2. User decision recorded 2026-06-25: preserve the requested operators and analysis so the discussion can continue later.

## Plan

1. Keep this SOW pending/open as the parked requirements ledger.
2. Resume only when the user explicitly asks to continue operator design or implementation.
3. Resolve all open decisions before any code changes.

## Delegation Plan

Implementer:

- No implementer is assigned while parked.

Reviewers:

- No reviewer gate is required while parked. When resumed for implementation, use the normal read-only reviewer pool after local validation.

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

- Any future implementer/reviewer failure must be recorded in this SOW before continuing.

## Execution Log

### 2026-06-25

- Created parked SOW from the user's operator-support question and the gap analysis discussed so far.

## Validation

Acceptance criteria evidence:

- Requirements, current support matrix, semantic questions, and recommended architecture are recorded in this SOW.

Tests or equivalent validation:

- No tests run; no code changed.

Real-use evidence:

- Not applicable for parked SOW creation; no runtime path changed.

Reviewer findings:

- No reviewer gate run; this is a parked analysis artifact, not an implementation.

Same-failure scan:

- Initial code scan found Rust and Go share the same exact-positive filter shape. A complete same-failure scan belongs to the future implementation phase.

Sensitive data gate:

- Durable evidence contains only operator names, repository paths, line references, and behavior summaries. No raw secrets, bearer tokens, SNMP communities, customer names, personal data, private endpoints, or customer-identifying IP addresses were recorded.

Artifact maintenance gate:

- AGENTS.md: no update needed for parked SOW creation.
- Runtime project skills: no update needed for parked SOW creation.
- Specs: no current behavior changed; spec update required only when implementation begins.
- End-user/operator docs: no current behavior changed; docs update required only when implementation ships.
- End-user/operator skills: no update needed.
- SOW lifecycle: pending/open parked SOW created.
- SOW-status.md: updated with this SOW.

Specs update:

- Not updated because this SOW does not change shipped behavior.

Project skills update:

- No project-skill update identified during SOW creation.

End-user/operator docs update:

- No docs update needed because this SOW is not executable yet and no behavior changed.

End-user/operator skills update:

- No end-user/operator skill impact identified during SOW creation.

Lessons:

- Operator work must be treated as query-planner design, not as small request parsing work, because negative and numeric predicates affect performance and semantics across the whole candidate set.

Follow-up mapping:

- All open semantic decisions are tracked in this SOW.

## Outcome

Parked. No implementation started.

## Lessons Extracted

Operator requirements are preserved for later discussion.

## Followup

- Resume only on explicit user request.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and later testing or use found broken behavior. Use a dated `## Regression - YYYY-MM-DD` heading at the end of the file. Never prepend regression content above the original SOW narrative.
