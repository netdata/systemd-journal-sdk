# SOW-0081 - systemd-journal Plugin And Facets Specification

## Status

Status: open

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: pending; created after SOW-0074 was killed and reverted.

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

- `netdata/netdata @ 7e9cbb5dab6f`
  - `src/collectors/systemd-journal.plugin/systemd-journal-execute.h`
  - `src/collectors/systemd-journal.plugin/systemd-journal.c`
  - `src/libnetdata/facets/facets.c`
  - `src/libnetdata/facets/facets.h`
  - `src/libnetdata/facets/logs_query_status.h`

Current state:

- SOW-0074 was killed because it optimized the wrong API shape.
- No full plugin/facets specification currently exists in this repository.

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

### 2026-06-02

- Created pending replacement SOW after SOW-0074 was killed.

## Validation

Pending.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

Pending.

## Regression Log

None yet.
