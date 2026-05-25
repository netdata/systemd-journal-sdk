# SOW-0019 - Forward Secure Sealing And Verification

## Status

Status: in-progress

Sub-state: active phase 1 - reference inventory and implementation guardrails.

## Requirements

### Purpose

Implement systemd journal Forward Secure Sealing file-format support and verification behavior in pure SDKs, without daemon-only lifecycle features.

### User Request

The final writer target includes Forward Secure Sealing where systemd journal files define it. SOW-0008 left FSS/full verification open because it adds cryptographic tag objects, key lifecycle, and verification semantics.

### Assistant Understanding

Facts:

- Current SDKs do not implement FSS tag object writing or full journal verification.
- Daemon-only journalctl operations remain out of scope.
- FSS support must remain pure-language and must not link to system journal libraries.

Inferences:

- Verification support should precede or ship with writer sealing support, because sealed files must be checked against stock behavior.
- Key lifecycle and sealing interval behavior need exact systemd reference inventory before implementation.

Unknowns:

- Whether every language has suitable pure cryptographic primitives for the exact FSS algorithms and state transitions systemd uses.
- How much daemon-only key setup behavior can be represented safely in file-backed SDK APIs.

### Acceptance Criteria

- A systemd FSS reference inventory records tag object format, key evolution, sealing interval behavior, verification behavior, and relevant upstream tests.
- Pure-language verification APIs validate sealed journal files and report controlled errors for tampering.
- Writers can emit sealed journal files with deterministic test keys and configurable sealing behavior where file-format rules allow it.
- Stock `journalctl --verify --verify-key` or equivalent stock verification passes for repository sealed files where applicable.
- Repository verification detects corrupted sealed data, missing tags, reordered data, and key mismatches.
- Daemon-only key-management commands are not implemented as journalctl daemon behavior.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/current/SOW-0008-20260523-interoperability-and-full-writer-features.md`
- `.agents/sow/specs/product-scope.md`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
- `src/libsystemd/sd-journal/`
- `test/units/TEST-04-JOURNAL*.sh`

Current state:

- Verification/FSS conformance cases are skipped or out of scope in earlier SOWs.
- Current writers produce unsealed journals.

Risks:

- Crypto implementation mistakes can create false security claims.
- Daemon lifecycle behavior can accidentally creep into a file-backed SDK project.
- Stock verification compatibility may depend on exact key encoding and tag placement.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The project has strong unsealed compatibility evidence, but FSS/full verification remains unimplemented. FSS is high-risk because it turns format compatibility into a cryptographic integrity contract.

Evidence reviewed:

- Product scope lists FSS in the final writer target.
- SOW-0008 records FSS/full verification as an open feature gap.
- Project scope excludes daemon lifecycle commands, so FSS must be implemented as file-backed SDK behavior only.
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
  `src/libsystemd/sd-journal/journal-def.h:140` defines 32-byte TAG HMAC length.
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
  `src/libsystemd/sd-journal/journal-def.h:142` defines `TagObject` as object header, `seqnum`, `epoch`, and SHA-256 HMAC tag.
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
  `src/libsystemd/sd-journal/journal-def.h:187` defines `HEADER_COMPATIBLE_SEALED` and `HEADER_COMPATIBLE_SEALED_CONTINUOUS`.
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
  `src/libsystemd/sd-journal/journal-def.h:248` defines the FSS sidecar header signature and fields.
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
  `src/libsystemd/sd-journal/journal-authenticate.c:44` appends TAG objects only for sealed journal headers.
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
  `src/libsystemd/sd-journal/journal-authenticate.c:267` defines exactly which object bytes enter the HMAC.
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
  `src/libsystemd/sd-journal/journal-authenticate.c:329` defines which immutable header byte ranges enter the HMAC.
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
  `src/libsystemd/sd-journal/journal-verify.c:840` requires a verification key for sealed files and returns `ENOKEY` without one.
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
  `src/libsystemd/sd-journal/journal-verify.c:1119` validates TAG sequence, epoch continuity, realtime boundaries, and HMAC equality.
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
  `src/libsystemd/sd-journal/fsprg.c:85` through `src/libsystemd/sd-journal/fsprg.c:415` defines deterministic seed expansion, prime generation, state layout, evolution, seeking, and key extraction.
- `tests/conformance/manifests/conformance-v01.json:193` already names `journal-verify-sealed`, but adapters skip it.
- `rust/src/adapter/main.rs:215`, `go/adapter/main.go:158`, `node/adapter/index.js:73`, and `python/adapter.py:81` show verification/FSS capability is currently disabled or skipped.

Affected contracts and surfaces:

- Writer tag object generation.
- Verification APIs and errors.
- journalctl rewrite `--verify` and FSS-related file-backed behavior where applicable.
- Test fixtures, corruption tests, and documentation.
- Key handling and sensitive data policies.

Existing patterns to reuse:

- Conformance fixture manifest skip handling.
- Stock `journalctl --verify --file` checks.
- Systemd test inventory approach from SOW-0003.
- Shared corruption fixture patterns.

Risk and blast radius:

- High. FSS touches cryptography, integrity claims, verification UX, and file-format state.

Sensitive data handling plan:

- Use deterministic synthetic test keys only. Never write private production keys, customer identifiers, secrets, or raw proprietary logs to durable artifacts. Redact key material in SOW logs unless it is a committed synthetic fixture key explicitly marked test-only.

Implementation plan:

1. Inventory systemd FSS algorithms, tag object layout, key derivation/evolution, verification, and tests.
2. Add a small internal FSS reference/vector layer so every language can prove the same FSPRG and HMAC bytes before journal integration.
3. Define file-backed SDK verification API and journalctl rewrite behavior.
4. Implement verification on generated sealed fixtures.
5. Implement writer sealing with deterministic test keys.
6. Add corruption/tamper tests and stock verification checks.
7. Update specs/docs and review with crypto/security emphasis.

Validation plan:

- Stock sealed fixtures verify or fail exactly as expected.
- Repository sealed files pass stock verification where applicable.
- Tamper tests fail deterministically.
- Existing unsealed matrices remain passing.
- External reviewers include explicit security and unwanted-side-effect review.

Artifact impact plan:

- AGENTS.md: no update expected unless FSS key handling requires a new project-wide guardrail.
- Runtime project skills: update if FSS validation becomes durable workflow.
- Specs: update product scope with exact FSS support.
- End-user/operator docs: update README and journalctl help/behavior docs.
- End-user/operator skills: no output/reference skill expected unless docs produce one.
- SOW lifecycle: pending until activated; may split inventory, verification, and writer sealing chunks.
- SOW-status.md: update when activated or closed.

Open-source reference evidence:

- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
- `src/libsystemd/sd-journal/`
- `test/units/TEST-04-JOURNAL*.sh`

Open decisions:

- None blocking activation. If exact FSS cryptographic behavior cannot be reproduced safely in one or more languages with pure dependencies, stop and present evidence before changing the final writer target.

## Implications And Decisions

1. FSS file-backed boundary
   - Decision: implement file-format FSS writing/verification, not daemon key-management commands.
   - Reason: daemon lifecycle operations are out of project scope.
   - Risk: API design must not imply daemon parity or operational key management beyond file-backed SDK behavior.

## Plan

1. Phase 1: finish a source-backed FSS inventory, derive deterministic FSPRG/HMAC vectors, and add repo-local guardrails that prevent unsafe live-journal validation.
2. Phase 2: implement pure verification primitives and repository verification APIs.
3. Phase 3: implement file-backed journalctl `--verify` / `--verify-key` behavior.
4. Phase 4: implement writer sealing with deterministic test keys and configurable sealing intervals.
5. Phase 5: add tamper/corruption fixtures, stock verification checks, docs/spec updates, and security review.

## Delegation Plan

Implementer:

- Preferred implementer is `llm-netdata-cloud/kimi-k2.6`, per the current project orchestration skill and user model routing decision.
- Fallback implementers are `llm-netdata-cloud/qwen3.6-plus`, then `llm-netdata-cloud/glm-5.1`, with any switch recorded here before use.

Reviewers:

- At least two reviewers from `llm-netdata-cloud/minimax-m2.7-coder`, `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`, and `llm-netdata-cloud/glm-5.1`, with prompts explicitly requesting crypto/security review.
- Reviewer prompts must be read-only and must include the SOW filename.

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

- Record implementer failure, reviewer failure, audit failure, crypto uncertainty, or model unavailability before changing plan or model.
- Record any attempted live-journal command as a blocker and revert only the specific unsafe change after user approval if it modified repository files.

## Execution Log

2026-05-25:

- Activated SOW-0019 after SOW-status showed no current SOW and listed this SOW as the next pending feature.
- Verified repository was clean at `a6c1972` before activation.
- Recorded systemd v260.1 evidence for TAG object layout, sealed header flags, FSS sidecar header, HMAC byte ranges, FSPRG state/key evolution, and verification behavior.
- Confirmed existing conformance cases and adapter capability skips for sealed verification and corruption verification.
- Added an explicit phase split so FSS vectors and guardrails land before high-risk writer sealing.
- Updated delegation model routing from the older Minimax implementer default to Kimi implementer plus reviewer-only pool.
- Safety constraint for implementers and reviewers: do not run `systemd-cat`, `logger`, `journalctl --setup-keys`, live `journalctl` without `--file` or repository-local `--directory`, `systemd-journal-remote --seal` against live journal data, or anything that writes `/var/log/journal` or `/run/log/journal`.

## Validation

Sensitive data gate:

- Pending phase validation. Current durable artifacts contain only synthetic test-key policy, source paths, line references, and model-routing notes. No production FSS keys, private logs, customer identifiers, or host journal data may be written to this SOW or committed fixtures.

Implementation validation:

- Pending implementation.

## Outcome

Pending.

## Lessons Extracted

Pending activation.

## Followup

Pending activation.

## Regression Log

None yet.
