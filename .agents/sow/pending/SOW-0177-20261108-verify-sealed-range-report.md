# SOW-0177 - Report The Validated Sealed Range From Verification

## Status

Status: open

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: tracked from SOW-0176 (item I7) on 2026-11-08; needs a public API decision.

## Requirements

### Purpose

Make `journalctl --verify --verify-key` print the same validated-range summary
as systemd v262 for sealed files.

### User Request

Follow-up of the SOW-0176 verify-key parity decision (I7 = A).

### Assistant Understanding

Facts:

- After `PASS`, v262 prints `=> Validated from %s to %s, final %s entries not
  sealed.`, `=> No sealing yet, %s of entries not sealed.` or `=> No sealing
  yet, no entries in file.` (systemd/systemd @ 8cc40e0c5e9234bf45084751ac53b1fbfe70b492
  `src/journal/journalctl-misc.c:70-85`).
- `VerifyFileWithKey` and `verify_file_with_key` return only success or failure.
  Both already track the realtime of the last verified TAG.

Inferences:

- Exposing the range needs a new public result type in both SDKs, plus
  `format_timespan` output in the CLIs.

Unknowns:

- The public API shape, decided at activation.

### Acceptance Criteria

- Both CLIs print the three v262 summary forms, compared against the pinned
  OpenSSL-enabled v262 oracle.
- The range comes from one public SDK function per language.

## Analysis

Sources checked: SOW-0176 keydiff results and the files cited above.

## Pre-Implementation Gate

Status: blocked

Problem / root-cause model:

- The verifier does not return the range v262 reports.

Evidence reviewed:

- See `## Requirements`.

Affected contracts and surfaces:

- Go and Rust verification APIs, both journalctl rewrites.

Existing patterns to reuse:

- `verifySealed` / `verify_sealed` tag tracking.

Risk and blast radius:

- Additive API; stderr output only.

Sensitive data handling plan:

- Synthetic fixtures only.

Implementation plan:

1. Decide the API, add it in both languages, print the summary, compare with the oracle.

Validation plan:

- `tests/interoperability/run_verify_matrix.py` and a stderr comparison.

Artifact impact plan:

- AGENTS.md: none expected.
- Runtime project skills: none expected.
- Specs: journalctl parity matrix.
- End-user/operator docs: API pages.
- End-user/operator skills: none.
- SOW lifecycle: pending until activated.

Open-source reference evidence:

- Cited above.

Open decisions:

- Public API shape.

## Implications And Decisions

None yet.

## Plan

See the implementation plan above.

## Execution Log

None yet.

## Validation

Pending.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None.
