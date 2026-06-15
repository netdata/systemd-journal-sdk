# SOW-0113 - Node.js Optional Native Mmap Reader

## Status

Status: open

Sub-state: follow-up from SOW-0111; tracking only, not activated.

## Requirements

### Purpose

Give Node.js consumers an explicit choice between the current pure
positioned-read reader and an optional mmap-backed reader without adding a
native runtime dependency to the default package.

### User Request

The user wants consumers of the API to decide which backend they use, while the
project supports both positioned-read and mmap options. The default Node.js
package must not silently gain a native mmap dependency.

### Assistant Understanding

Facts:

- Node.js core does not expose a portable mmap API.
- The current default Node.js SDK package uses bounded positioned-read windows.
- SOW-0111 removes mmap from the default-package TypeScript supported mode
  surface while runtime explicit `mmap` selection continues to fail clearly.

Inferences:

- Native mmap support should live behind an optional package, optional peer
  dependency, or equivalent explicit opt-in boundary.
- The optional backend must preserve the bounded reader-memory and row-lifetime
  contracts before it can be presented as production-capable.

Unknowns:

- Which package/API shape best satisfies dependency, packaging, platform, and
  security constraints.
- Whether available npm mmap packages meet the no-native-default-runtime
  contract and support the target Linux, FreeBSD, macOS, and Windows matrix.

### Acceptance Criteria

- A dependency review selects or rejects candidate native mmap packages with
  evidence.
- The chosen API/package boundary keeps the default SDK import pure and free of
  native mmap runtime loading.
- Node.js consumers can explicitly choose positioned-read or mmap when the
  optional backend is installed and supported.
- Tests prove fallback semantics, row-lifetime behavior, small-window eviction,
  unsupported-platform behavior, and TypeScript/runtime agreement.
- Documentation makes the dependency boundary and platform support explicit.

## Analysis

Sources checked:

- SOW-0111 findings.
- `node/src/lib/reader-access.js`
- `node/index.d.ts`
- `node/README.md`

Current state:

- Default Node.js reader access is bounded positioned-read.
- Explicit `mmap` selection is rejected by runtime.
- SOW-0111 keeps the default package from advertising mmap as available until an
  optional backend exists and is validated.

Risks:

- Native addons can break installation, packaging, security review, and the
  project's pure default package expectation.
- A silently optional dependency can create platform-specific behavior drift.
- Mmap-backed Buffer lifetime has to preserve current-row semantics under
  window eviction and file refresh.

## Pre-Implementation Gate

Status: needs-user-decision

Problem / root-cause model:

- Node.js needs consumer choice, but core Node.js lacks a portable mmap API.
  Supporting mmap therefore requires an explicit dependency and package/API
  boundary decision.

Evidence reviewed:

- SOW-0111 records the user decision to split optional Node.js mmap support.
- `node/src/lib/reader-access.js` currently implements only positioned-read
  windows and rejects explicit mmap.

Affected contracts and surfaces:

- Node.js package metadata, TypeScript declarations, runtime reader access
  abstraction, tests, benchmarks, docs, and release packaging.

Existing patterns to reuse:

- `node/src/lib/reader-access.js` bounded accessor abstraction.
- SOW-0111 default-package TypeScript/runtime agreement.
- Existing Node.js unsupported-access error behavior.

Risk and blast radius:

- Medium to high packaging risk because native addons can fail at install time
  or load time.
- Medium API risk because backend selection must stay explicit.
- Medium performance risk because mmap must improve or justify itself versus
  bounded positioned reads.

Sensitive data handling plan:

- Use synthetic repository fixtures only.
- Do not record real journal payloads, host identities, credentials, customer
  data, or private endpoints.

Implementation plan:

1. Review candidate mmap packages and package-boundary options.
2. Return to the user with numbered options and a recommendation before code
   changes.
3. Implement only the selected optional backend/package design.

Validation plan:

- Dependency review and platform matrix evidence.
- Node.js unit tests, TypeScript tests, reader-access tests, docs validation,
  benchmark comparison, and SOW audit.

Artifact impact plan:

- AGENTS.md: likely unaffected unless the dependency policy changes.
- Runtime project skills: update only if a durable Node optional-native-backend
  workflow is created.
- Specs: update product scope with the final optional backend contract.
- End-user/operator docs: update Node README and wiki docs.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: remains pending until explicitly activated.
- SOW-status.md: add as pending follow-up.

Open-source reference evidence:

- Not checked yet. This SOW begins with dependency/package research.

Open decisions:

1. Optional backend boundary:
   - A. Optional dependency in the default package, loaded only when requested.
   - B. Separate package that registers/provides the mmap backend.
   - C. No native mmap until a maintained dependency meets the platform and
     packaging requirements.

## Implications And Decisions

- Created as a follow-up from the SOW-0111 user decision. No implementation
  decision has been made yet.

## Plan

1. Research candidate packages and package-boundary options.
2. Present evidence-backed options to the user.
3. Implement the selected design in a later activation.

## Delegation Plan

Implementer:

- Pending activation.

Reviewers:

- Pending activation.

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

- Record dependency, validation, reviewer, or audit failures in this SOW before
  proceeding.

## Execution Log

### 2026-06-15

- Created as pending follow-up tracking from SOW-0111.

## Validation

Acceptance criteria evidence:

- Pending.

Tests or equivalent validation:

- Pending.

Real-use evidence:

- Pending.

Reviewer findings:

- Pending.

Same-failure scan:

- Pending.

Sensitive data gate:

- Pending.

Artifact maintenance gate:

- AGENTS.md: pending.
- Runtime project skills: pending.
- Specs: pending.
- End-user/operator docs: pending.
- End-user/operator skills: pending.
- SOW lifecycle: pending SOW created.
- SOW-status.md: pending update.

Specs update:

- Pending.

Project skills update:

- Pending.

End-user/operator docs update:

- Pending.

End-user/operator skills update:

- Pending.

Lessons:

- Pending.

Follow-up mapping:

- Pending.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.
