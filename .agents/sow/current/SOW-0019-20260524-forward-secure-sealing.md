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

2026-05-25 (Phase 1 fix — FSPRG_Seek direct exercise):

- Patched `tests/fss/fsprg_vector_generator.c` to:
  - Fix hex buffer size calculation (was sized for max(msk,mpk) but state is larger).
  - For each epoch, compute both an `evolved_state` (via `FSPRG_Evolve` from `state0`) and a `seek_state` (via `FSPRG_Seek` from `state0` using `msk` and `seed`).
  - Compare the two states byte-for-byte; abort with `-EIO` if they differ.
  - Emit `"seek_state_hex"` and `"seek_matches_evolved": true` in every epoch object.
- Regenerated fixture with `./tests/fss/run_vectors.sh --update`.
- Updated `tests/fss/README.md` to state that `FSPRG_Seek` is directly exercised and explain the cross-check semantics.
- Exact commands run:
  - `./tests/fss/run_vectors.sh --update` – built and regenerated fixture successfully
  - `python3 -m json.tool tests/fss/fixtures/fsprg-vectors-v01.json` – valid JSON
  - Custom Python check confirmed every epoch has `seek_state_hex`, `seek_matches_evolved: true`, and `state_hex == seek_state_hex`

2026-05-25 (Phase 1 implementation):

- Created `tests/fss/` directory for repo-local FSS reference/vector area.
- Added `tests/fss/fsprg_vector_generator.c` – C helper that calls systemd internal FSPRG APIs (`FSPRG_GenMK`, `FSPRG_GenState0`, `FSPRG_Evolve`, `FSPRG_GetKey`) and emits deterministic JSON vectors. Uses `_exit(0)` after `fflush(stdout)` to avoid a libgcrypt atexit-handler crash with dynamic gcrypt loading; all output is flushed before exit.
- Added `tests/fss/build.sh` – clones or reuses systemd v260.1 under `.local/`, copies the generator into the systemd source tree, patches `src/libsystemd/meson.build` to add a manual test target, and builds it with ninja. Modeled after `tests/datasets/ingesters/systemd/build.sh`.
- Added `tests/fss/fixtures/fsprg-vectors-v01.json` – committed deterministic fixture covering:
  - `FSPRG_RECOMMENDED_SECPAR = 1536`
  - `FSPRG_RECOMMENDED_SEEDLEN = 12`
  - Two fixed synthetic seeds: all-zeros and incremental `0x01..0x0c`
  - Generated `msk_hex`, `mpk_hex`, `state0_hex`
  - Epochs `0, 1, 2, 3, 17` with full state and 32-byte `FSPRG_GetKey` output for `idx = 0, 1`
- Added `tests/fss/run_vectors.sh` – runner that builds the helper, generates vectors to a temp file under `.local/`, validates JSON, and compares to the committed fixture. Supports `--update` mode to refresh the fixture.
- Added `tests/fss/README.md` – documents what the vectors prove, which systemd source lines define the behavior, why daemon key setup is out of scope, and a safe/unsafe command list for this repo.
- Exact commands run during implementation:
  - `./tests/fss/build.sh` – built `test-fss-vector-generator` inside `.local/systemd-v260.1-build/`
  - `.local/systemd-v260.1-build/test-fss-vector-generator > tests/fss/fixtures/fsprg-vectors-v01.json` – generated fixture
  - `./tests/fss/run_vectors.sh` – validated compare mode passes
  - `.agents/sow/audit.sh` – passed
- Files changed (all within repository):
  - `tests/fss/fsprg_vector_generator.c` (new)
  - `tests/fss/build.sh` (new)
  - `tests/fss/run_vectors.sh` (new)
  - `tests/fss/fixtures/fsprg-vectors-v01.json` (new)
  - `tests/fss/README.md` (new)
  - `.local/systemd-v260.1-src/src/libsystemd/sd-journal/test-fss-vector-generator.c` (generated copy, under `.local/`, not committed)
  - `.local/systemd-v260.1-src/src/libsystemd/meson.build` (patched in `.local/` clone only, not committed)

2026-05-25 (Phase 1 reproducibility fix — build.sh marker):

- Fixed `tests/fss/build.sh` to be self-contained on a fresh systemd v260.1 clone.
- The original script used `sd-journal/test-dataset-ingester.c` as the meson.build insertion marker, but that entry is not upstream; it is added by `tests/datasets/ingesters/systemd/build.sh`. This made the FSS build script dependent on prior local state.
- Changed the marker to the upstream `sd-journal/test-journal-append.c` manual test entry, matching the pattern used by `tests/datasets/ingesters/systemd/build.sh` for its own insertion.
- The script remains idempotent: the `if "sd-journal/test-fss-vector-generator.c" not in text:` guard prevents duplicate entries on repeated runs.
- Exact commands run to validate:
  - `./tests/fss/run_vectors.sh` – `[PASS] Generated fixture matches committed fixture.`
  - `python3 -m json.tool tests/fss/fixtures/fsprg-vectors-v01.json` – valid JSON
  - `.agents/sow/audit.sh` – passed with no errors
  - `grep 'test-journal-append\|test-dataset-ingester' tests/fss/build.sh` – confirms only `test-journal-append` is referenced
- Files changed:
  - `tests/fss/build.sh` (modified)

2026-05-25 (Reviewer round 1 disposition and cleanup):

- Four read-only reviewers returned `PRODUCTION GRADE` for Phase 1.
- A cleanup implementer retry with Kimi stalled after reading files and was terminated with targeted process cleanup; no partial changes were observed from that run.
- Non-blocking findings were still cleaned up to avoid carrying technical debt:
  - `tests/fss/fsprg_vector_generator.c` now checks `fflush(stdout)` before `_exit(EXIT_SUCCESS)`. Flush failure reports a stderr error, flushes stderr, and exits with `_exit(EXIT_FAILURE)`.
  - The generator `fail:` path now flushes stderr and exits through `_exit(EXIT_FAILURE)`, avoiding the documented libgcrypt atexit path on failure too.
  - `tests/fss/run_vectors.sh` now captures the generator path printed by `tests/fss/build.sh` instead of hardcoding `.local/systemd-v260.1-build/test-fss-vector-generator`.
  - `tests/fss/run_vectors.sh` writes generator output to a candidate `.local/*.tmp` file, validates JSON there, and promotes it to the stable generated fixture path only after validation succeeds.
- Reviewer findings accepted as non-blocking and intentionally not changed:
  - Variable-length arrays in the helper are acceptable for the fixed recommended secpar vector size.
  - Shared `.local/systemd-v260.1-src` and `.local/systemd-v260.1-build` mirrors the existing dataset-ingester pattern; Meson/Ninja regeneration handles patched `meson.build` state.
  - Shellcheck SC2059 remains consistent with the existing visible-command helper pattern in this repository.

2026-05-25 (Reviewer round 2 disposition and cleanup):

- Second full-scope reviewer round returned `PRODUCTION GRADE` from Minimax, Mimo, Qwen, and GLM.
- Qwen identified two low-severity helper-quality findings:
  - if the second seed generation failed after the first seed printed, stdout could contain an invalid partial JSON document before the helper exited non-zero;
  - per-epoch evolved/seeked FSPRG states used variable-length arrays.
- Both findings were cleaned instead of carried as debt:
  - `tests/fss/fsprg_vector_generator.c` now buffers each seed object with `open_memstream()` and prints the top-level JSON only after both seeds generate successfully;
  - evolved and seeked states are now heap buffers allocated once per seed and reused across epochs.
- GLM noted that `.local/fsprg-vectors-generated.json` remains after successful compare mode. This is accepted as non-blocking because the file is under gitignored `.local/`, mirrors the existing generated-artifact inspection pattern, and helps inspect drift when compare mode fails.
- Post-cleanup `./tests/fss/run_vectors.sh` passed and confirmed the committed fixture bytes did not change.

2026-05-25 (Reviewer round 3 closeout):

- Third full-scope reviewer round re-reviewed the whole Phase 1 changed scope after the per-seed buffering and heap-state cleanup.
- Minimax, Mimo, Qwen, and GLM all returned `PRODUCTION GRADE` with no required fixes before commit.
- Non-blocking observations accepted and recorded:
  - Shellcheck SC2059 remains an informational finding inherited from the existing visible-command helper pattern.
  - The shell helper's first error line prints `$1`, while the immediately following `Full command` line prints `$*`; this is cosmetic and matches the existing dataset-ingester helper pattern.
  - `run_vectors.sh` extracts the final `build.sh` stdout line with `tail -n 1`; this is accepted because `build.sh` intentionally prints the executable path last.
  - Shared `.local/systemd-v260.1-src` and `.local/systemd-v260.1-build` remain accepted because both helper scripts have independent Meson entry guards and mirror the existing dataset-ingester design.

### Gaps and Risks Discovered (Phase 1)

1. Libgcrypt atexit segfault: the dynamic-loading path used by systemd v260.1 crashes during program exit. The `_exit` workaround is documented but means the helper cannot use normal `return from main` cleanup. This is acceptable for a test helper but would be unacceptable for production code.
2. The vectors cover only the recommended secpar/seedlen. Other valid secpar values (multiples of 16 from 16 to 16384) are not vectored. Future SDK implementations must still validate `ISVALID_SECPAR` behavior independently.
3. `FSPRG_Seek` is directly exercised for every epoch vector. The generator cross-checks evolved state against seeked state and aborts on mismatch. (Fixed 2026-05-25; see Execution Log.)
4. HMAC tag object bytes and header authentication bytes are inventoried in the Pre-Implementation Gate but not yet represented as executable vectors. Those require journal file integration, which is intentionally deferred to Phase 2+.

## Validation

Sensitive data gate:

- Phase 1 validated. No production keys, customer identifiers, secrets, or host journal data were written to durable artifacts. The committed fixture contains only synthetic test vectors with explicitly documented seeds (all-zeros and incremental `0x01..0x0c`). All key material is deterministic and reproducible from public parameters.

Implementation validation:

- Vector generator builds successfully from systemd v260.1 internal code and exits cleanly.
- Generated JSON validates with `python3 -m json.tool`.
- `./tests/fss/run_vectors.sh` (compare mode) passes: generated fixture is byte-identical to committed fixture.
- `.agents/sow/audit.sh` passes with no errors.
- Shellcheck run on `tests/fss/build.sh` and `tests/fss/run_vectors.sh` reports only SC2059 (info) about printf format strings, which matches the existing project pattern in `tests/datasets/ingesters/systemd/build.sh`.
- No changes were made outside this repository. The systemd source clone and build remain under `.local/`.

Seek-fix validation (2026-05-25):

- `FSPRG_Seek` is called for every epoch (`0, 1, 2, 3, 17`) in both seeds.
- The generator compares evolved state and seeked state byte-for-byte; mismatch aborts with non-zero exit.
- Every epoch object in the fixture includes `seek_state_hex` and `seek_matches_evolved: true`.
- A Python spot-check confirms `state_hex == seek_state_hex` for all 10 epoch entries.
- The hex buffer size bug (original allocation used `max(msklen, mpklen)` but `statelen` is larger) was fixed by sizing against `max(msklen, mpklen, statelen)`.

Reviewer-cleanup validation (2026-05-25):

- `./tests/fss/run_vectors.sh` passed after the runner began capturing the generator path and promoting only JSON-valid candidate output.
- `python3 -m json.tool tests/fss/fixtures/fsprg-vectors-v01.json` passed.
- Python structural check confirmed all 10 epoch entries still have matching `state_hex` and `seek_state_hex`.
- `.agents/sow/audit.sh` passed.
- `git diff --check -- .agents/sow/current/SOW-0019-20260524-forward-secure-sealing.md tests/fss` passed.

Reviewer-round-2 cleanup validation (2026-05-25):

- `./tests/fss/run_vectors.sh` passed after per-seed output buffering and heap-state cleanup.
- The generated fixture remained byte-identical to `tests/fss/fixtures/fsprg-vectors-v01.json`.
- `python3 -m json.tool tests/fss/fixtures/fsprg-vectors-v01.json` passed.
- Python structural check confirmed all 10 epoch entries still have matching `state_hex` and `seek_state_hex`.
- `git diff --check -- .agents/sow/current/SOW-0019-20260524-forward-secure-sealing.md tests/fss` passed.
- Grep for personal-name, absolute workstation path, and unfinished-work marker terms in changed durable files returned no matches.

Reviewer-round-3 validation (2026-05-25):

- Four read-only reviewers rechecked the full Phase 1 scope after cleanup and returned `PRODUCTION GRADE`.
- No reviewer required additional code, fixture, documentation, or SOW changes before this Phase 1 checkpoint commit.
- Reviewers independently confirmed the fixture remains trustworthy as a systemd v260.1 FSPRG baseline and that Phase 1 does not claim SDK verification or writer sealing support.

## Outcome

Phase 1 completed:

- A repo-local FSS reference/vector area exists under `tests/fss/`.
- A systemd v260.1 C reference helper builds and runs deterministically under `.local/`.
- Committed fixture `tests/fss/fixtures/fsprg-vectors-v01.json` captures FSPRG behavior for recommended parameters, two seeds, five epochs, and two key indices.
- A runner script supports compare mode (default) and explicit update mode.
- Documentation covers vector semantics, upstream source references, scope boundaries, and safety guardrails.

Phase 2 (pure verification primitives) can now proceed with a trusted baseline.

## Lessons Extracted

1. Libgcrypt dynamic loading via systemd's `dlopen_gcrypt()` path can segfault in atexit handlers when the program exits normally. Using `_exit(0)` after explicit `fflush(stdout)` is an acceptable workaround for a test helper that has no heap resources requiring cleanup beyond what glibc reclaims.
2. Building inside the systemd meson tree is the lowest-friction way to get correct headers, defines, and linking for internal APIs like FSPRG. Copying the helper into the tree and adding a `manual` test entry matches the existing dataset-ingester pattern.
3. Deterministic synthetic vectors should be generated once, committed, and then protected by a compare-mode runner. This prevents silent drift when build environments or library versions change.

## Followup

- Phase 2: implement pure-language FSPRG primitives and journal verification APIs.
- Phase 3: implement file-backed journalctl `--verify` / `--verify-key` behavior.
- Phase 4: implement writer sealing with deterministic test keys.
- Phase 5: add tamper/corruption fixtures, stock verification checks, docs/spec updates, and security review.

## Regression Log

None yet.
