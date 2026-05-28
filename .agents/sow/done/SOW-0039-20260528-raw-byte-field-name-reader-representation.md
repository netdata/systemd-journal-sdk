# SOW-0039 - RAW Byte Field Name Reader Representation

## Status

Status: closed

Sub-state: closed on 2026-05-28 because the user agreed to fold this work into
the broader reader parity SOW. The implementation requirement is now tracked by
SOW-0043.

## Requirements

### Purpose

Ensure RAW-mode journal files with field names that are valid at the on-disk
DATA/FIELD structure layer can be represented consistently by SDK reader APIs.

### User Request

The user required RAW mode to allow everything the journal file structures
allow, subject to mandatory structural rules such as non-empty names and no
`=` in field names. Review later found a reader representation gap for
non-UTF8 field names in string-keyed reader maps.

### Assistant Understanding

Facts:

- RAW writer policy accepts byte-oriented field names where language APIs can
  represent them.
- Node.js and Python readers preserve raw DATA payload bytes, but their
  convenience field maps use strings.
- The user agreed on 2026-05-28 to fold this into the reader parity plan.

Inferences:

- Reader byte-name representation belongs with libsystemd/`jf` reader parity,
  not as an isolated implementation SOW.

Unknowns:

- The final byte-preserving reader API shape will be decided and implemented in
  SOW-0043.

### Acceptance Criteria

- SOW-0043 includes byte-preserving RAW reader representation requirements.
- This SOW is moved to done with `Status: closed`.

## Analysis

Sources checked:

- `node/src/lib/reader.js`
- `python/journal/reader.py`
- `.agents/sow/current/SOW-0038-20260528-field-name-policy-layers.md`

Current state:

- Superseded by SOW-0043.

Risks:

- Leaving this pending would duplicate reader parity scope and make scheduling
  unclear.

## Pre-Implementation Gate

Status: not applicable - superseded before implementation

Problem / root-cause model:

- The original gap is real, but its correct home is the reader parity SOW.

Evidence reviewed:

- Existing SOW-0039 evidence and user-approved restructuring plan.

Affected contracts and surfaces:

- Reader entry representation, directory readers, query/export helpers, and
  journalctl rewrites.

Existing patterns to reuse:

- Existing payload arrays that preserve raw `FIELD=value` bytes.

Risk and blast radius:

- Low for closing this SOW. Implementation risk transfers to SOW-0043.

Sensitive data handling plan:

- This SOW contains only synthetic field-name planning. No sensitive data is
  required.

Implementation plan:

1. Close this SOW as superseded.
2. Ensure SOW-0043 carries the requirement.

Validation plan:

- SOW audit verifies closed status and done-directory placement.

Artifact impact plan:

- AGENTS.md: no update needed.
- Runtime project skills: no update needed.
- Specs: no update here; SOW-0043 owns spec changes.
- End-user/operator docs: no update here; SOW-0043 owns docs.
- End-user/operator skills: no output/reference skill affected.
- SOW lifecycle: move this SOW to done with `Status: closed`.
- SOW-status.md: remove from pending and reference SOW-0043.

Open-source reference evidence:

- No external open-source source was checked for the closure. The work is
  transferred to SOW-0043.

Open decisions:

- None. User agreed to fold this into reader parity.

## Implications And Decisions

1. 2026-05-28 supersession
   - Decision: close SOW-0039 and track the implementation under SOW-0043.
   - Implication: RAW byte-name reader work will be designed with the full
     reader API rather than as a narrow map-key patch.

## Plan

1. Move this SOW to `.agents/sow/done/`.
2. Add RAW byte-name reader requirements to SOW-0043.

## Delegation Plan

Implementer:

- No implementation in this SOW.

Reviewers:

- SOW-0043 owns implementation review.

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

- If audit fails, repair the lifecycle/status issue before committing.

## Execution Log

### 2026-05-28

- Closed as superseded by SOW-0043 after user agreement.

## Validation

Acceptance criteria evidence:

- SOW-0043 tracks RAW byte-preserving reader representation.

Tests or equivalent validation:

- SOW audit validates status/directory consistency.

Real-use evidence:

- Not applicable to this closed planning SOW; SOW-0043 owns real validation.

Reviewer findings:

- Not applicable; no implementation.

Same-failure scan:

- Not applicable; no implementation.

Sensitive data gate:

- No raw secrets, credentials, bearer tokens, SNMP communities, customer names,
  personal data, non-private customer-identifying IPs, private endpoints, or
  proprietary incident details were added.

Artifact maintenance gate:

- AGENTS.md: no update needed.
- Runtime project skills: no update needed.
- Specs: SOW-0043 owns spec changes.
- End-user/operator docs: SOW-0043 owns docs changes.
- End-user/operator skills: no output/reference skill affected.
- SOW lifecycle: moved to done as closed.
- SOW-status.md: updated by restructuring commit.

Specs update:

- SOW-0043 owns spec updates.

Project skills update:

- No project skill update needed.

End-user/operator docs update:

- SOW-0043 owns docs updates.

End-user/operator skills update:

- No output/reference skill update needed.

Lessons:

- RAW reader representation should be designed with the whole reader API.

Follow-up mapping:

- Tracked by SOW-0043.

## Outcome

Closed as superseded by SOW-0043.

## Lessons Extracted

Reader representation gaps should be grouped with reader parity so every
language and facade is aligned at once.

## Followup

- SOW-0043 - Rust Reader Libsystemd/Jf Parity.

## Regression Log

None yet.
