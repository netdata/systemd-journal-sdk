# SOW-0081 - systemd-journal Plugin And Facets Specification

## Status

Status: completed

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: completed.

## Requirements

### Purpose

Create a complete, evidence-backed specification of how Netdata's `systemd-journal.plugin` and facets library currently work, so the SDK can implement the same query business logic deliberately instead of approximating it from partial benchmarks.

### User Request

The user asked to replace SOW-0074 with a first SOW that documents `systemd-journal.plugin` plus facets in full detail, including nuances such as over-budget sampling, timestamp delta handling because users care about realtime timestamps rather than row commit timestamps, native journal filtering, facets, histogram, anchor/direction, and Top-N rows.

### Assistant Understanding

Facts:

- Netdata's current `systemd-journal.plugin` reads journal files and feeds fields into the facets library.
- The result model includes Top-N rows, selected facet counters for the query timeframe, and a histogram for one selected field over the query timeframe.
- The plugin uses native journal matches for exact filters where available.
- The facets library contains business logic beyond simple counting, including sampling, FTS, selected values, row retention, row anchoring, histogram slots, and output formatting.

Inferences:

- A correct SDK replacement needs a precise behavioral spec before implementation.
- The spec must separate journal/file-format mechanics from Netdata/facets business logic.
- Some behavior may be intentionally Netdata-specific and should be labeled as such instead of becoming a generic SDK default.

Unknowns:

- The complete list of subtle facets behaviors must be extracted from the code.
- Whether every existing behavior should become part of the generic SDK API, or whether some should remain Netdata integration policy, may require later user decisions.

### Acceptance Criteria

- Produce a committed specification document under `.agents/sow/specs/` describing the current plugin/facets query model in detail.
- Capture input/query parameters, defaults, output model, Top-N row semantics, anchor and direction behavior, timeframe calculation, realtime timestamp adjustment, native filter setup, OR/AND filter semantics, negative filters if present, FTS behavior, facets, histogram, sampling, over-budget behavior, progress/cancellation/timeouts, file/source selection, and error/discrepancy handling.
- Include file:line evidence from Netdata source for every major behavior.
- Include a glossary of terms used by the later SDK API SOWs.
- Identify which behaviors are generic log-explorer semantics and which are Netdata-specific integration policy.
- Identify implementation risks and open decisions for SOW-0082.
- Update `SOW-status.md`.

## Analysis

Sources checked:

- `ktsaou/netdata @ b695fa41f8ef`
  - `src/collectors/systemd-journal.plugin/systemd-journal.c`
  - `src/collectors/systemd-journal.plugin/systemd-journal-execute.h`
  - `src/collectors/systemd-journal.plugin/systemd-journal-function.h`
  - `src/collectors/systemd-journal.plugin/systemd-journal-sampling.h`
  - `src/collectors/systemd-journal.plugin/systemd-internals.h`
  - `src/collectors/systemd-journal.plugin/systemd-journal-files.c`
  - `src/collectors/systemd-journal.plugin/systemd-journal-annotations.c`
  - `src/libnetdata/facets/facets.c`
  - `src/libnetdata/facets/facets.h`
  - `src/libnetdata/facets/logs_query_status.h`
  - `src/libnetdata/facets/README.md`
- `netdata/netdata @ 5d611c4ce8c2`
  - checked for divergence; `facets.c`, `facets.h`, and `facets/README.md`
    are byte-identical to the fork, while the plugin wrapper and
    `logs_query_status.h` differ, so the fork is the integration authority for
    this SOW.

Current state:

- SOW-0074 was killed because it optimized the wrong API shape.
- `.agents/sow/specs/systemd-journal-plugin-facets.md` now records the
  plugin/facets behavior and separates generic explorer semantics from
  Netdata-specific policy.

Findings:

- Code is authoritative over the existing Netdata facets README for the default
  timeframe. The README says 15 minutes, but the code uses one hour.
- Current plugin behavior has three known performance costs that SOW-0082 must
  address: unnecessary compressed DATA expansion, no early stop for row field
  traversal once requested fields are satisfied, and repeated processing of
  reusable DATA objects.

Risks:

- Missing a business-rule nuance could make the Rust implementation fast but semantically incompatible with Netdata.
- Copying Netdata-specific policy into a generic SDK API without labeling it could overfit the SDK.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The prior explorer SOW tried to design and benchmark an API before fully specifying the legacy behavior it was meant to improve. That allowed row-only and partial-facet benchmarks to become acceptance evidence even though they did not fully model the target query behavior.

Evidence reviewed:

- User correction that the target is a legacy-like optimized API producing optional histogram, optional facets, and Top-N rows with indexed filters and optional FTS.
- Netdata source paths listed above.

Affected contracts and surfaces:

- Future Rust explorer API contract.
- Future Netdata `systemd-journal.plugin` integration plan.
- SDK specs and benchmark design.

Existing patterns to reuse:

- Existing SOW spec files under `.agents/sow/specs/`.
- Existing open-source evidence citation format in project SOWs.

Risk and blast radius:

- Documentation-only SOW with low code blast radius.
- High downstream design impact because SOW-0082 and SOW-0083 depend on this spec.

Sensitive data handling plan:

- Only source-code evidence and sanitized behavior summaries are recorded.
- No real journal payloads, hostnames, IPs, usernames, customer identifiers, or private paths are written to durable artifacts.

Sensitive data gate:

- Durable artifacts must contain only sanitized source-code evidence, upstream
  repository identities, commits, repository-relative paths, and line
  references. They must not contain raw journal payloads, hostnames, IPs,
  usernames, customer identifiers, secrets, credentials, private endpoints, or
  workstation-private paths.

Implementation plan:

1. Read the plugin, facets, and query-status code paths end to end.
2. Produce `.agents/sow/specs/systemd-journal-plugin-facets.md`.
3. Record evidence with upstream repository identity and line references.
4. Extract open decisions and requirements for SOW-0082.

Validation plan:

- Same-failure search for unrecorded code paths using relevant function and symbol names.
- `git diff --check`.
- `.agents/sow/audit.sh`.
- Whole-SOW reviewer pass if the user wants external review before SOW-0082 starts.

Artifact impact plan:

- AGENTS.md: no expected update.
- Runtime project skills: update only if a durable workflow rule is discovered.
- Specs: add the new plugin/facets specification.
- End-user/operator docs: no expected update.
- End-user/operator skills: no expected update.
- SOW lifecycle: complete only after spec and status update are committed.
- SOW-status.md: update with pending/current/completed state.

Open-source reference evidence:

- Netdata source evidence is required and must be recorded as `netdata/netdata @ <commit>` with repository-relative paths.

Open decisions:

- None blocking SOW-0081. Any behavior that needs a product decision for SOW-0082 must be recorded in the spec.

## Implications And Decisions

1. 2026-06-02 replacement decision
   - Decision: SOW-0074 is killed and replaced by a spec-first sequence.
   - Implication: no further implementation should proceed until the legacy plugin/facets behavior is specified.

## Plan

1. Read and trace Netdata plugin/facets code.
2. Write the specification.
3. Validate evidence coverage and update status.

## Delegation Plan

Implementer:

- Local implementation unless the user explicitly re-enables external implementers.

Reviewers:

- Reviewer pool after complete specification and local validation if needed: minimax, kimi, qwen, glm, and mimo.

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

- If the source behavior is ambiguous, record the ambiguity with evidence and ask for a user decision before SOW-0082 implementation.

## Execution Log

### 2026-06-05

- Moved to current after SOW-0092 completed and Rust reader performance
  hardening closed.
- Traced current Netdata `systemd-journal.plugin` and facets behavior from
  `ktsaou/netdata @ b695fa41f8ef`.
- Added `.agents/sow/specs/systemd-journal-plugin-facets.md`.
- Recorded the code-vs-README default timeframe discrepancy and the SOW-0082
  requirements extracted from the trace.

### 2026-06-02

- Created pending replacement SOW after SOW-0074 was killed.

## Validation

Acceptance criteria evidence:

- Added `.agents/sow/specs/systemd-journal-plugin-facets.md`.
- The spec records query parameters, defaults, output model, Top-N behavior,
  anchor/direction behavior, timeframe handling, realtime timestamp adjustment,
  native slice filtering, OR/AND match semantics, FTS, facets, histogram,
  sampling, progress, timeout, cancellation, file/source selection, and status
  handling.
- The spec includes line-level evidence from `ktsaou/netdata @ b695fa41f8ef`
  and comparison context from `netdata/netdata @ 5d611c4ce8c2`.
- The spec includes a glossary, separates generic explorer semantics from
  Netdata-specific policy, and extracts requirements plus open risks for
  SOW-0082.
- `.agents/sow/SOW-status.md` was updated to move SOW-0081 from Pending to
  Current during implementation.

Local validation:

- `git diff --check`: passed.
- `.agents/sow/audit.sh`: passed with verdict
  `SOW initialization complete and clean`.
- Sensitive-data scan for the changed SOW/spec/status artifacts found no raw
  journal payloads, hostnames, IPs, usernames, customer identifiers, secrets,
  credentials, private endpoints, or workstation-private paths.

Reviewer findings and dispositions:

- `llm-netdata-cloud/minimax-m2.7-coder`: production-grade content review with
  a lifecycle warning that the SOW move/spec/status changes were not committed
  yet and closeout sections were pending. Disposition: accepted; final close
  records validation/outcome/follow-up and commits the move/spec/status update
  together.
- `llm-netdata-cloud/kimi-k2.6`: PRODUCTION GRADE. Finding: citation for
  case-insensitive `SIMPLE_PATTERN` matching included `simple_pattern.h:63`,
  which is only the header guard terminator. Disposition: fixed by removing
  that citation and retaining the accurate `simple_pattern.h:29`,
  `simple_pattern.c:47-68`, `simple_pattern.c:194-214`, and `facets.c:1783`
  evidence.
- `llm-netdata-cloud/qwen3.6-plus`: PRODUCTION GRADE. Findings: validation,
  outcome, lessons, and follow-up were still pending. Disposition: accepted;
  closeout sections are filled before completion.
- `llm-netdata-cloud/glm-5.1`: PRODUCTION GRADE. Findings: timeout line range
  was broad and closeout sections were pending. Disposition: accepted; timeout
  ownership and constants are explicitly recorded, and closeout sections are
  filled before completion.
- `llm-netdata-cloud/mimo-v2.5-pro`: NOT PRODUCTION GRADE for lifecycle
  completion only; spec content was verified as production grade. Findings:
  validation, outcome, lessons, and follow-up sections were pending.
  Disposition: accepted; this closeout fills those sections before completion.

Final pre-close reviewer pass:

- The reviewer pool re-ran the same whole-SOW review after closeout sections
  were filled and the stale `simple_pattern.h:63` citation was fixed.
- All reviewers verified the spec content as production-grade.
- Non-blocking evidence-hygiene findings were accepted and fixed before close:
  `systemd-main.c` was added to the spec evidence list, `simple_pattern.c`
  case-insensitive helper lines were added to the FTS citation, the
  recalibration citation was narrowed, the histogram evidence range was
  extended, and the over-budget sampling path was named explicitly.
- Lifecycle-only findings about changing `Status:` to `completed`, moving the
  SOW to `done/`, and updating `.agents/sow/SOW-status.md` are resolved by the
  final closeout commit.

Same-failure search:

- Searched the spec/SOW for stale `Pending`, `simple_pattern.h:63`, and
  imprecise `FACET_MAX_VALUE_LENGTH` references after fixes.
- Searched the changed artifacts for personal names, local workstation paths,
  raw IP-like values, secrets, and token-like values; no changed-artifact issue
  was found.

Artifact maintenance gate:

- `AGENTS.md`: no update needed; this SOW changed product/spec knowledge, not
  project workflow.
- Runtime project skills: no update needed; no new how-to workflow rule was
  discovered.
- Specs: updated by adding
  `.agents/sow/specs/systemd-journal-plugin-facets.md`.
- End-user/operator docs: no update needed; this is internal SDK planning
  evidence before public API implementation.
- End-user/operator skills: no update needed; no exported user-facing skill was
  affected.
- SOW lifecycle: SOW-0081 moved from `pending/` to `current/` during work and
  is moved to `done/` on completion.
- `.agents/sow/SOW-status.md`: updated for current state and must be updated
  again on completion.

Follow-up mapping:

- SOW-0082 tracks the optimized Rust explorer API that consumes this spec.
- SOW-0083 tracks later index-derived facet and histogram optimization.
- No untracked deferred item remains in this SOW.

## Outcome

- Delivered a source-backed specification for Netdata
  `systemd-journal.plugin` plus facets behavior at
  `.agents/sow/specs/systemd-journal-plugin-facets.md`.
- Confirmed code is authoritative over the existing facets README for the
  default timeframe: code uses one hour, while the README says fifteen minutes.
- Identified the three legacy performance costs SOW-0082 must eliminate:
  unnecessary compressed DATA expansion, no early field traversal stop once
  requested data is satisfied, and repeated processing of reusable journal DATA
  objects.
- Established the boundary between generic SDK explorer semantics and
  Netdata-specific adapter policy.

## Lessons Extracted

- Specification work must verify README/operator documentation claims against
  source before treating defaults as facts. This SOW found a real
  code-vs-README discrepancy in the default query timeframe.
- The optimized explorer API needs to be specified against the full
  plugin/facets business model, not against partial row-scan benchmark shapes.
- Netdata provider wrapper names must not leak into SDK design as dependencies;
  SDK APIs should express journal-native operations directly.

## Followup

- SOW-0082 - Rust Optimized Journal Explorer API: open and depends on this
  specification.
- SOW-0083 - Index-Derived Facet And Histogram Optimization: open and depends
  on the SOW-0082 API shape.
- No other follow-up is deferred from SOW-0081.

## Regression Log

None yet.
