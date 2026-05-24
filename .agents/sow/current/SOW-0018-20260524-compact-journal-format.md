# SOW-0018 - Compact Journal Format

## Status

Status: in-progress

Sub-state: activated after SOW-0021 completion; user decision requires compact support and caller-selectable writer format.

## Requirements

### Purpose

Add systemd compact journal format support where applicable, with shared reader/writer validation and explicit compatibility evidence.

### User Request

The user requires pure SDKs that read and write journal files according to systemd journal rules. SOW-0008 confirmed all current writers create regular non-compact journals; compact format remains a final writer-target gap.

### Assistant Understanding

Facts:

- Current Rust, Go, Node.js, and Python writers create regular, non-compact journal files.
- Current accepted reader slices either reject compact journals or do not claim compact support.
- Compact journals change object layout and offset widths, so they are not a small flag-only feature.
- User decision on 2026-05-25: compact format support is required, and writers must expose an option allowing callers to choose regular or compact output.

Inferences:

- Reader support and fixture inventory should come before writer support.
- Byte-identical compact output may need to wait for deterministic dataset and ingester work.

Unknowns:

- Exact compact fixture coverage available in systemd v260.1 and whether additional generated fixtures are required.

### Acceptance Criteria

- A systemd compact-format reference inventory records exact object layout, header flags, reader behavior, writer behavior, and tests/fixtures used.
- Readers in Rust, Go, Node.js, and Python handle compact journals or return controlled unsupported errors until implementation is complete.
- Writers can emit compact journals only after all reader and stock-tool compatibility gates pass.
- Shared fixtures and interoperability tests cover compact journals across every language.
- Stock `journalctl --verify --file`, stock reads, stock libsystemd reads, and repository readers pass for compact files written by repository writers.
- Writer APIs in Rust, Go, Node.js, and Python expose an explicit caller option for regular vs compact output; the default must remain regular unless a SOW records a user decision to change defaults.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/current/SOW-0008-20260523-interoperability-and-full-writer-features.md`
- `.agents/sow/specs/product-scope.md`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
- `src/libsystemd/sd-journal/journal-def.h`
- `test/journal-data/`
- `test/test-journals/`

Current state:

- Regular non-compact journal support is the current cross-language writer/read surface.
- Compact format is listed as not implemented in product scope and interoperability docs.

Risks:

- Compact layout changes can create silent reader misparsing if treated as a minor variant.
- Writer support before reader support can produce files that repository tooling cannot diagnose.
- Byte identity may expose allocation/layout deltas after semantic compatibility is achieved.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The final writer target includes regular and compact journal formats where applicable, but current SDKs only support regular journals. Compact support needs a format inventory and staged reader/writer rollout.

Evidence reviewed:

- Product scope says current writers are regular, non-compact.
- SOW-0008 records compact format as an open writer feature gap.
- systemd journal definitions include compact-related object layout mechanics that need direct reference inventory.

Affected contracts and surfaces:

- Journal header flags and object layout parsing.
- Reader iteration, matching, export, JSON, and cursor behavior.
- Writer object construction and offset arrays.
- Interoperability and conformance fixtures.
- File-backed journalctl behavior.

Existing patterns to reuse:

- Systemd test inventory approach from SOW-0003.
- Shared conformance manifests and fixture runners.
- Interoperability matrix structure from SOW-0008.

Risk and blast radius:

- High. Compact layout affects core parser and writer object code in all languages.

Sensitive data handling plan:

- Use upstream fixtures and synthetic generated files only. Record upstream paths and commits, not workstation paths. No sensitive runtime data expected.

Implementation plan:

1. Inventory compact format from systemd source, docs, and fixtures.
2. Add compact fixture coverage and controlled unsupported behavior tests.
3. Implement reader support per language.
4. Implement writer support only after reader support is verified.
5. Add compact interoperability matrix and update docs/specs.

Validation plan:

- Compact fixture tests per language.
- Cross-language compact writer/reader matrix.
- Stock journalctl/libsystemd verification for generated compact files.
- Existing regular, binary, compression, live, and lock matrices remain passing.
- External reviewers inspect parser/writer blast radius.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: update only if compact-specific validation becomes durable workflow.
- Specs: update product scope with exact compact support state.
- End-user/operator docs: update README feature matrices.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: pending until activated; may split reader and writer chunks if needed.
- SOW-status.md: update when activated or closed.

Open-source reference evidence:

- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
- `src/libsystemd/sd-journal/journal-def.h`
- `test/journal-data/`
- `test/test-journals/`

Open decisions:

- None blocking activation. If compact writer support proves too broad, split reader and writer SOWs with evidence.

## Implications And Decisions

1. Reader-before-writer compact rollout
   - Decision: compact reader support and fixtures must precede compact writer claims.
   - Reason: writers that produce files repository readers cannot inspect would weaken the project debugging surface.
   - Risk: compact writer delivery may need multiple chunks.

2. Caller-selectable writer format
   - Decision: compact journal support is required, and every writer must expose an option allowing callers to choose regular or compact output.
   - Reason: compact format has a different on-disk layout and a 4 GiB file-size ceiling, so it is not safe as an implicit or silent default change.
   - Implication: writer options, directory writer propagation, livewriter/ingester fixture flags, and docs must distinguish regular vs compact output.
   - Risk: if defaults change accidentally, existing byte-identical regular writer validation and downstream expectations can regress.

## Plan

1. Inventory systemd compact format and fixtures.
2. Implement reader handling and conformance fixtures.
3. Implement writer support.
4. Extend interoperability matrices.
5. Review, validate, and update specs/docs.

## Delegation Plan

Implementer:

- Preferred implementer is `llm-netdata-cloud/kimi-k2.6`.

Reviewers:

- Reviewer pool is `llm-netdata-cloud/minimax-m2.7-coder`, `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`, and `llm-netdata-cloud/glm-5.1`.

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

- Record implementer failure, reviewer failure, audit failure, fixture gaps, or model unavailability before changing plan or model.

## Execution Log

### 2026-05-25

- Activated after SOW-0021 completed and was pushed.
- Recorded user decision: compact format is required, and writers must expose explicit regular/compact selection while preserving regular output as the default unless changed by a later user decision.
- Updated delegation metadata to use Kimi as implementer and Minimax as reviewer-only per current project routing.

## Validation

Pending implementation.

Acceptance criteria evidence:

- Pending implementation.

Tests or equivalent validation:

- Pending implementation.

Real-use evidence:

- Pending implementation.

Reviewer findings:

- Pending implementation and review.

Same-failure scan:

- Pending implementation.

Sensitive data gate:

- Use upstream public source references, public package metadata if dependencies are added, synthetic fixtures, and generated local test artifacts only. Durable artifacts must not include raw secrets, credentials, private endpoints, customer identifiers, or personal data.

Artifact maintenance gate:

- AGENTS.md: pending; update only if compact support changes project-wide workflow or guardrails.
- Runtime project skills: pending; update if compact validation becomes a durable workflow future agents must run.
- Specs: pending; update `product-scope.md` with exact shipped compact support state.
- End-user/operator docs: pending; update interoperability/docs feature matrices after implementation.
- End-user/operator skills: none expected unless compact support creates exported operator workflow artifacts.
- SOW lifecycle: active in `.agents/sow/current/`; close only after status is `completed`, file moves to `.agents/sow/done/`, audit passes, and implementation plus lifecycle change are committed together.
- SOW-status.md: updated on activation; update again on close.

Specs update:

- `product-scope.md` updated on activation to record the explicit regular/compact writer selection target.

## Outcome

Pending implementation.

## Lessons Extracted

Pending implementation.

## Followup

Pending implementation.

## Regression Log

None yet.
