# SOW-0112 - Netdata Sampling Contract Clarification

## Status

Status: completed

Sub-state: completed as a documentation/spec clarification with no Rust or Go
implementation changes.

## Requirements

### Purpose

Clarify the Netdata function sampling and slice contract so future SDK work is
fit for Netdata dashboard and explorer use: exact row paging must stay exact,
large delta analysis must stay bounded, and the SDK should keep optimized
slice-style semantics instead of preserving the C plugin's slower fallback
behavior.

### User Request

The user asked for the agreed sampling and slice decisions to be written down,
and confirmed that the current Rust and Go SDK behavior does not need code
changes.

### Assistant Understanding

Facts:

- Current Rust and Go match each other on the Explorer/Netdata sampling engine.
- Current Rust and Go sample `data_only=true, delta=true` requests.
- Current Rust and Go disable sampling for `data_only=true, delta=false`.
- Current Rust and Go always use SDK indexed slice semantics at the Netdata
  boundary.
- The C plugin disables sampling for all `data_only` requests.
- The C plugin can disable slice mode and fall back to full traversal.

Inferences:

- The remaining differences are product contract choices, not Rust/Go parity
  bugs.
- The spec should state the chosen SDK contract clearly enough that future
  reviewers do not reopen this as an implementation gap.

Unknowns:

- None blocking this documentation clarification.

### Acceptance Criteria

- SOW records the user decision before spec edits.
- `.agents/sow/specs/systemd-journal-plugin-facets.md` states the SDK sampling
  contract for full analysis, data-only paging, and data-only delta.
- The spec states that always-indexed slice semantics are intentional SDK
  behavior and that `slice:false` compatibility is not a Rust/Go implementation
  requirement.
- Validation records that no Rust or Go implementation change is required.

## Analysis

Sources checked:

- `rust/src/journal/src/explorer.rs`
- `rust/src/journal/src/netdata.rs`
- `go/journal/explorer.go`
- `go/journal/netdata.go`
- `go/journal/explorer_test.go`
- `go/journal/netdata_test.go`
- `.agents/sow/specs/systemd-journal-plugin-facets.md`
- `ktsaou/netdata @ 36050079cfa9`
  - `src/collectors/systemd-journal.plugin/systemd-journal-sampling.h`
  - `src/collectors/systemd-journal.plugin/systemd-journal-execute.h`
  - `src/libnetdata/facets/logs_query_status.h`

Current state:

- Rust and Go implement the same budget-based sampling model.
- Rust and Go already match the desired SDK behavior:
  - sampling disabled for exact data-only row paging;
  - sampling enabled for data-only delta analysis;
  - always-indexed slice semantics at the SDK Netdata boundary.

Risks:

- Without a clear spec note, future agents may treat the C plugin's
  all-data-only sampling disablement as a Rust/Go bug.
- Without a clear spec note, future agents may try to reintroduce `slice:false`
  traversal behavior and its different facet counting semantics into the SDK.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The initial gap analysis mixed C, Rust, and Go evidence. The safer model is
  two-step: C plugin to Rust, then Rust to Go. That exposes two C-to-SDK
  differences, but the user decided both are intentional SDK contract choices.

Evidence reviewed:

- C disables sampling for all data-only requests in
  `systemd-journal-sampling.h`.
- Rust and Go enable sampling only when analysis is enabled, where analysis is
  full query or data-only delta.
- Rust and Go keep data-only non-delta row paging exact and unsampled.
- Rust and Go always use indexed slice semantics at the SDK boundary.

Affected contracts and surfaces:

- Netdata function request semantics.
- Explorer sampling semantics.
- SDK specs for future Rust, Go, Python, and Node parity work.
- Comparator expectations for data-only, delta, sampling, and slice behavior.

Existing patterns to reuse:

- `.agents/sow/specs/systemd-journal-plugin-facets.md` already separates C
  plugin behavior from SDK-specific Netdata boundary policy.
- SOW-0095, SOW-0107, and SOW-0109 already use Rust as the SDK behavior
  reference for cross-language parity.

Risk and blast radius:

- Low implementation risk because no code changes are planned.
- Medium documentation risk if the wording implies exact C plugin behavior
  where the SDK deliberately diverges.

Sensitive data handling plan:

- Use only source paths, line-level behavior summaries, and synthetic test
  evidence.
- Do not record logs, credentials, tokens, SNMP communities, customer names,
  personal data, private endpoints, or production incident details.

Implementation plan:

1. Record the user decision in this SOW.
2. Clarify the sampling and slice contract in the Netdata function facets spec.
3. Run focused text checks and SOW audit if the SOW is closed in this turn.

Validation plan:

- Search the spec for the updated sampling/slice wording.
- Confirm Rust/Go code changes are not present.
- Run `.agents/sow/audit.sh` if closing the SOW.

Artifact impact plan:

- AGENTS.md: no update expected; this is not a project-wide workflow change.
- Runtime project skills: no update expected; this is product behavior, not
  working procedure.
- Specs: update expected in
  `.agents/sow/specs/systemd-journal-plugin-facets.md`.
- End-user/operator docs: no update expected; this is an internal contract
  clarification unless a later docs SOW publishes the detail.
- End-user/operator skills: no output/reference skills affected.
- SOW lifecycle: current SOW records and validates the decision.
- SOW-status.md: update expected because a SOW is created.

Open-source reference evidence:

- `ktsaou/netdata @ 36050079cfa9`
  - `src/collectors/systemd-journal.plugin/systemd-journal-sampling.h`
  - `src/collectors/systemd-journal.plugin/systemd-journal-execute.h`
  - `src/libnetdata/facets/logs_query_status.h`

Open decisions:

- Resolved by user in this SOW; see `## Implications And Decisions`.

## Implications And Decisions

1. Data-only without delta
   - Decision: sampling is disabled.
   - Reasoning: the request means "return exact next/previous rows around this
     anchor or window"; sampling has no useful meaning for exact row paging.
   - Implication: returned rows are exact, and the SDK should not estimate or
     skip row payload analysis for this mode.
   - Risk: none for large analysis because this mode does not request analysis
     output.

2. Data-only with delta
   - Decision: sampling is enabled when requested and when valid sampling
     bounds exist.
   - Reasoning: delta analysis may cover a day or a week if a dashboard was not
     refreshed for a long time, so `facets_delta`, `histogram_delta`, and
     `items_delta` need the same bounded-cost sampling and estimation safety as
     full analysis.
   - Implication: returned row candidates remain exact and fully processed, but
     delta analysis counters may include `[unsampled]` and `[estimated]`.
   - Risk: this intentionally differs from the current C plugin, which disables
     sampling for all data-only requests.

3. `slice:false`
   - Decision: the SDK Netdata boundary keeps always-indexed slice semantics
     and does not need to implement the C plugin's `slice:false` fallback.
   - Reasoning: `slice:false` exists for the C plugin's brute-force fallback
     path and changes facet semantics in a way the SDK should not preserve as
     its contract.
   - Implication: the SDK may echo or normalize `slice:true` and report
     optimized slice-style facet results.
   - Risk: callers that expected the C fallback's same-field filter exclusion
     behavior should not rely on the SDK Netdata boundary for that legacy mode.

4. Rust and Go implementation status
   - Decision: current Rust and Go SDK code needs no change for this issue.
   - Reasoning: Rust and Go already implement the chosen SDK contract.
   - Implication: this SOW is documentation/spec clarification only.

## Plan

1. Update the Netdata function facets spec with the resolved sampling and slice
   contract.
2. Validate the text and confirm no Rust/Go code change is needed.
3. Leave any broader consumer-documentation publication to the existing docs
   SOW queue unless the user requests it here.

## Delegation Plan

Implementer:

- Not delegated. This is a narrow user-decision recording and spec wording
  update by the project manager.

Reviewers:

- No external reviewer batch planned because no code behavior changes and the
  user decision is explicit.

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

- If validation finds contradictory spec or code evidence, stop and ask the
  user before changing implementation behavior.

## Execution Log

### 2026-06-15

- Created this SOW to record the user decision before spec edits.
- Updated the Netdata function facets spec and validated the recorded contract.
- Closed the SOW after confirming no Rust or Go implementation change is
  required.

## Validation

Acceptance criteria evidence:

- User decision recorded in `## Implications And Decisions` before the spec
  clarification:
  - data-only without delta disables sampling and keeps exact row paging;
  - data-only with delta may sample analysis outputs while returned rows remain
    exact;
  - `slice:false` fallback semantics are not part of the SDK contract;
  - current Rust and Go implementation needs no change.
- Spec text updated in
  `.agents/sow/specs/systemd-journal-plugin-facets.md` to state the same SDK
  contract for sampling, delta analysis, returned rows, and indexed slice
  semantics.

Tests or equivalent validation:

- Focused Rust sampling validation passed:
  `cargo test -q -p systemd-journal-sdk 'sampling' -- --nocapture`.
- Focused Go sampling validation passed:
  `go test ./journal -run 'TestExplorerSampling|TestNetdata.*Sampling' -count=1`.
- SOW audit passed: `.agents/sow/audit.sh`.

Real-use evidence:

- Not needed; this SOW clarifies existing behavior and makes no SDK runtime
  changes.

Reviewer findings:

- No external reviewers planned; no code behavior changes.

Same-failure scan:

- Text scan confirmed the resolved contract appears in the SOW, canonical spec,
  and SOW ledgers:
  `rg -n 'data-only|Data-only|slice:false|current Rust/Go|current Rust and Go|SOW-0112' .agents/sow/current/SOW-0112-20260615-netdata-sampling-contract-clarification.md .agents/sow/specs/systemd-journal-plugin-facets.md .agents/sow/SOW-status.md SOW-status.md`.
- `git status --short` confirms no Rust or Go source files were modified by this
  clarification.

Sensitive data gate:

- Clean. The SOW and spec record source paths, behavior summaries, and test
  command evidence only. No credentials, tokens, customer data, private
  endpoints, production logs, or personal data were written.

Artifact maintenance gate:

- AGENTS.md: no update needed; no workflow or repository-wide guardrail changed.
- Runtime project skills: no update needed; this is product contract wording,
  not a change to how agents work in the repository.
- Specs: updated
  `.agents/sow/specs/systemd-journal-plugin-facets.md`.
- End-user/operator docs: no update needed; this clarification is internal
  Netdata compatibility/spec memory and does not change public API usage.
- End-user/operator skills: no output/reference skill is affected.
- SOW lifecycle: SOW completed and moved to `done/` with matching
  `Status: completed`.
- SOW-status.md: updated both the canonical ledger and root convenience ledger
  to show SOW-0112 as completed.

Specs update:

- Updated `.agents/sow/specs/systemd-journal-plugin-facets.md`.

Project skills update:

- None needed. The decision changes the recorded SDK contract, not project
  operating procedure.

End-user/operator docs update:

- None needed. There is no public-facing behavior change and no SDK code change.

End-user/operator skills update:

- None needed. No output/reference skills are maintained for this behavior.

Lessons:

- For C/Rust/Go parity checks, analyze C to Rust first and Rust to Go second.
  That separates intentional SDK contract choices from real cross-language
  implementation drift.

Follow-up mapping:

- No follow-up SOW is needed for Rust or Go. The current implementation already
  matches the accepted SDK contract.

## Outcome

Decision recorded. The current Rust and Go SDK implementation needs no changes
for this sampling and slice issue.

## Lessons Extracted

- Future sampling reviews should treat data-only non-delta and data-only delta
  as different request classes. The first is exact row paging; the second can be
  large-range analysis and needs bounded-cost sampling.

## Followup

None.

## Regression Log

None yet.
