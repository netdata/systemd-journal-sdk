# SOW-0030 - Monotonic Writer Validity

## Status

Status: completed

Sub-state: Implemented, validated, reviewed, and ready to commit. Split from SOW-0022 Gap 8. User decision recorded: follow Netdata vendored behavior.

## Requirements

### Purpose

Prevent SDK writers from producing journal files that stock `journalctl --verify --file` rejects because same-boot monotonic timestamps go backwards.

### User Request

Resolve writer correctness gaps before performance work and Netdata integration, with the same rules and API behavior across Rust, Go, Node.js, and Python.

### Assistant Understanding

Facts:

- systemd verification rejects decreasing monotonic timestamps for entries with the same boot ID.
- Low-level single-file writers currently accept explicit caller monotonic timestamps and update tail metadata without rejecting same-boot regressions.
- Current high-level `Log` writers clamp non-progressing realtime and monotonic overrides forward; `product-scope.md` documents that behavior.
- SOW-0022 recorded a user decision that writers must reject appends that make same-boot monotonic timestamps go backwards.

Inferences:

- The intended policy needs to distinguish low-level raw writers from high-level ingestion writers, or the current high-level contract must change.

Unknowns:

- None. The user selected Netdata vendored behavior.

### Acceptance Criteria

- A user decision resolves the reject-versus-clamp API policy before implementation.
- All four languages implement the same policy for low-level writers and high-level directory writers.
- Negative tests prove stock systemd rejects intentionally corrupted backward-monotonic fixtures.
- Positive writer tests prove SDK-generated files pass stock verification under the accepted policy.

## Analysis

Sources checked:

- `SOW-0022-20260525-compatibility-test-gap-audit.md`
- `product-scope.md`
- `go/journal/writer.go`
- `go/journal/log.go`
- `rust/src/crates/journal-core/src/file/writer.rs`
- `rust/src/crates/journal-log-writer/src/log/mod.rs`
- `node/src/lib/writer.js`
- `node/src/lib/directory-writer.js`
- `python/journal/writer.py`
- `python/journal/directory_writer.py`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`

Current state:

- Low-level writers use explicit monotonic timestamps without same-boot regression checks.
- High-level writers clamp non-progressing monotonic overrides forward.
- Verification APIs already detect monotonic regressions in generated/corrupted files.

Risks:

- Rejecting in high-level ingestion APIs may break callers that rely on clamping for messy source timestamps.
- Clamping everywhere can hide timestamp mutation in raw writer APIs.
- Allowing raw backward monotonic writes can produce files stock systemd rejects.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- There is a policy conflict between strict "writers reject invalid same-boot monotonic order" and the current high-level directory writer contract that clamps unsafe timestamp overrides forward for ingestion safety.

Evidence reviewed:

- `go/journal/writer.go:245-253`
- `go/journal/writer.go:710-721`
- `go/journal/log.go:834-843`
- `rust/src/crates/journal-core/src/file/writer.rs:446-519`
- `rust/src/crates/journal-log-writer/src/log/mod.rs:637-642`
- `rust/src/crates/journal-log-writer/src/log/mod.rs:461-471`
- `node/src/lib/writer.js:245-252`
- `node/src/lib/directory-writer.js:447-451`
- `python/journal/writer.py:262-267`
- `python/journal/directory_writer.py:457-460`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `src/libsystemd/sd-journal/journal-verify.c:1050-1063`

Affected contracts and surfaces:

- Low-level writer append APIs.
- High-level directory `Log` append APIs.
- Netdata ingestion writer API.
- Verification tests and docs.

Existing patterns to reuse:

- Current high-level clamping behavior and tests.
- Netdata vendored low-level raw writer behavior.
- Current verification monotonic-regression checks.
- Stock `journalctl --verify --file` oracle.

Risk and blast radius:

- Medium public API risk because behavior is caller-visible.
- High compatibility risk if raw writers remain able to produce invalid files silently.

Sensitive data handling plan:

- Use synthetic timestamps and synthetic fields only.

Implementation plan:

1. Record the user decision.
2. Add cross-language tests for low-level and high-level behavior under the accepted policy.
3. Implement same-boot monotonic validity checks or clamp behavior consistently.
4. Update docs/specs to remove any ambiguity.

Validation plan:

- Run per-language writer tests.
- Run stock `journalctl --verify --file` on accepted outputs.
- Run negative corruption fixture tests against stock systemd and repository verification APIs.

Artifact impact plan:

- AGENTS.md: no expected update.
- Runtime project skills: no expected update unless this becomes a mandatory validation rule.
- Specs: update `product-scope.md` to document exact low-level and high-level policy.
- End-user/operator docs: update SDK API docs if behavior changes.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: status remains open until the user decision is recorded.
- SOW-status.md: update on activation and completion.

Open-source reference evidence:

- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
  - `src/libsystemd/sd-journal/journal-verify.c`

Open decisions:

1. Same-boot backward monotonic policy:
   - Option A: Low-level writers reject explicit same-boot backward monotonic timestamps; high-level `Log` writers keep documented clamp behavior.
     - Pros: raw APIs do not silently write invalid files; ingestion APIs remain tolerant and Netdata-friendly.
     - Cons: two API layers have different behavior.
     - Risk: docs/tests must make the distinction impossible to miss.
   - Option B: All writer APIs reject explicit same-boot backward monotonic timestamps.
     - Pros: simplest strict rule.
     - Cons: breaks the current high-level `Log` clamp contract and can make ingestion callers handle timestamp normalization themselves.
     - Risk: higher integration friction for SNMP traps, NetFlow, and OTEL-style ingestion.
   - Option C: All writer APIs clamp backward explicit monotonic timestamps.
     - Pros: files remain valid and ingestion is forgiving.
     - Cons: raw APIs mutate caller-provided timestamps.
     - Risk: hidden data distortion.
   - Recommendation: Option A, because it preserves the current high-level ingestion contract while preventing low-level raw APIs from writing invalid files silently.

## Implications And Decisions

- User decision on 2026-05-26: "do whatever netdata does."
- Accepted policy:
  - High-level `Log` / directory writer APIs clamp non-progressing entry realtime and entry monotonic timestamps forward to preserve strict progression.
  - Low-level raw single-file writer APIs accept caller-provided `realtime` and `monotonic` values without rejecting or clamping.
- Evidence from Netdata vendored Rust:
  - High-level clamp: `src/crates/journal-log-writer/src/log/mod.rs:246-256`.
  - Cross-restart high-level seed from same-boot tail monotonic: `src/crates/journal-log-writer/src/log/mod.rs:269-271` and `src/crates/journal-log-writer/src/log/chain.rs:122-141`.
  - High-level clamp tests: `src/crates/journal-log-writer/tests/log_writer.rs:480-521` and `src/crates/journal-log-writer/tests/log_writer.rs:555-614`.
  - Low-level raw pass-through: `src/crates/journal-core/src/file/writer.rs:182-220` and tail publication in `src/crates/journal-core/src/file/writer.rs:260-279`.
- Implications:
  - This is intentionally different from the previous SOW-0022 strict-reject wording.
  - Low-level raw writers remain capable of producing files that stock systemd verification can reject if callers supply backward same-boot monotonic timestamps.
  - High-level ingestion APIs stay Netdata-compatible and must continue producing stock-verifiable files by clamping unsafe overrides.

## Plan

1. Record the user decision.
2. Add behavior tests before code changes.
3. Implement policy in all languages.
4. Validate with stock systemd and read-only reviewers.

## Delegation Plan

Implementer:

- Local implementation by the project manager, per current routing decision.

Reviewers:

- Read-only reviewers from the active pool after implementation: minimax, kimi, qwen, glm. Mimo is skipped.

Failure handling:

- Record policy or validation failures here before changing scope.

## Execution Log

- 2026-05-26: Activated SOW after user approved proceeding. Current policy target is the recorded Netdata behavior: high-level `Log` / directory writer clamp; low-level raw writer pass-through.
- 2026-05-26: Added strict verification checks so Rust, Go, Node.js, and Python reject same-boot backward monotonic entry ordering during repository verification.
- 2026-05-26: Added raw low-level writer tests proving explicit zero monotonic pass-through and intentionally backward same-boot monotonic fixture rejection by repository verification and stock `journalctl --verify --file`.
- 2026-05-26: Updated high-level directory writers so Go, Node.js, and Python seed the monotonic clamp floor from a persisted chain tail only when the tail boot ID matches the current writer boot ID. Rust already had the required `tail_monotonic_for_boot` behavior.
- 2026-05-26: Aligned explicit zero semantics across languages:
  - Rust high-level APIs use `Option<u64>` where `Some(0)` is explicit.
  - Go added `RealtimeUsecSet` and `MonotonicUsecSet` to distinguish explicit zero from omitted zero-value struct fields.
  - Node.js and Python use timestamp option key presence to distinguish explicit zero from omitted defaults.
- 2026-05-26: Added high-level tests for explicit zero monotonic clamping, explicit zero realtime clamping, and cross-boot monotonic clamp non-seeding.
- 2026-05-26: Updated `product-scope.md`, `go/API.md`, and Rust, Go, Node.js, and Python READMEs with the accepted timestamp policy.
- 2026-05-26: Ran full validation and two external read-only review rounds. Final reviewer consensus: `PRODUCTION GRADE`.

## Validation

Acceptance criteria evidence:

- User decision recorded in `## Implications And Decisions`: follow Netdata vendored behavior.
- All four languages now preserve explicit caller timestamps in low-level raw writers while high-level directory writers clamp unsafe ingestion timestamps.
- Negative tests create intentional same-boot backward monotonic raw files and require repository verification failure plus stock `journalctl --verify --file` failure when stock tooling is available.
- Positive high-level writer tests call stock `journalctl --verify --file` on generated files where the test environment has journalctl.

Commands run after implementation:

- `journalctl --version | head -n 1`
  - Result: `systemd 260 (260.1-2-manjaro)`.
- `gofmt -w go/journal/log_test.go`
  - Result: success.
- `cargo fmt --all` from `rust/`
  - Result: success.
- `go test ./...` from `go/`
  - Result: pass.
- `cargo test` from `rust/`
  - Result: pass.
- `node test/all.js` from `node/`
  - Result: pass.
- `.local/python-venv/bin/python python/test_all.py`
  - Result: pass.

Reviewer findings and dispositions:

- Round 1 reviewers: minimax, glm, kimi, and qwen all reached `PRODUCTION GRADE`; kimi found a real cross-language explicit-zero monotonic drift and missing Rust cross-boot test.
  - Disposition: fixed explicit-zero monotonic behavior and test coverage across Rust, Go, Node.js, and Python; added Rust cross-boot high-level test.
- Round 1 non-blocking Rust strict verifier double-parse observation.
  - Disposition: accepted as a low-risk verification-only inefficiency; not tracked as a separate follow-up because SOW-0009 covers performance work and this path is not ingestion hot-path code.
- Round 2 reviewers: minimax, qwen, glm, and kimi all reached `PRODUCTION GRADE`.
  - Disposition: qwen reported a missing Rust explicit-zero-realtime test, but local evidence showed `test_entry_realtime_override_is_clamped_monotonic` now uses `with_entry_realtime_usec(0)` and asserts `first + 1`; treated as false positive.
  - Disposition: kimi noted Python explicit `None` now fails fast under key-presence semantics; accepted because `None` is not documented as an omitted timestamp marker and the new contract intentionally uses key presence.
  - Disposition: glm and kimi noted Rust verification double-parses entries; accepted as above.
  - Disposition: kimi noted Go `EntryOptions` field insertion and compound `Set || value != 0` logic; accepted because all in-repo uses are named-field initializers and the logic preserves backward-compatible non-zero timestamp behavior.

Same-failure search:

- Searched for stale `non-zero monotonic` wording and updated the remaining API/documentation hit in `go/API.md`.
- Reviewers checked all four raw writer paths, high-level writer paths, verification paths, and cross-boot tail boot-ID seeding paths.

Sensitive data gate:

- Test values use synthetic UUIDs, timestamps, and field values only.
- No secrets, customer data, personal data, or production identifiers were added.

Artifact maintenance gate:

- `AGENTS.md`: no update needed; workflow and guardrails did not change.
- Runtime project skills: no update needed; this SOW shipped product behavior and tests, not a new work procedure.
- Specs: `product-scope.md` updated with current timestamp policy.
- End-user/operator docs: `go/API.md` and all four language READMEs updated.
- End-user/operator skills: none exist for this project surface.
- SOW lifecycle: this SOW is marked `completed` and moved to `done/` as part of the closing commit.
- `SOW-status.md`: updated to remove SOW-0030 from current work and list it as done.
- SOW audit: `.agents/sow/audit.sh` passed after the SOW move and status update.

Spec update:

- `product-scope.md` now records low-level raw pass-through, high-level clamp behavior, explicit zero handling, and same-boot tail seeding policy.

Project skill update:

- No skill update needed. The compatibility rule already requires stock verification and cross-language consistency; this SOW implements a product policy under those existing rules.

End-user/operator docs update:

- `go/API.md`, `go/README.md`, `rust/README.md`, `node/README.md`, and `python/README.md` updated.

Follow-up mapping:

- No new follow-up SOW is required.
- SOW-0009 remains the existing benchmark/profile/optimization SOW and covers any later performance investigation if verification overhead becomes relevant.

## Outcome

Completed.

Rust, Go, Node.js, and Python now share the same writer timestamp policy:

- Low-level raw writers preserve explicit caller-provided realtime and monotonic timestamps, including explicit zero monotonic.
- High-level directory `Log` writers clamp non-progressing realtime and same-boot monotonic overrides forward, including explicit zero realtime and explicit zero monotonic.
- High-level writers seed the monotonic clamp floor from the persisted chain tail only when the tail entry boot ID matches the current writer boot ID.
- Repository verification APIs reject same-boot backward monotonic entries consistently across all four languages.
- Stock `journalctl --verify --file` is used as the external oracle for positive generated files and intentional negative raw fixtures.

## Lessons Extracted

Lessons:

- The raw-writer and high-level-writer API layers must remain explicitly documented as different contracts: raw APIs preserve caller data for fixture and low-level use, while ingestion APIs normalize timestamps for valid output.
- Go zero-value structs need explicit `Set` flags for zero timestamp overrides; dynamic-language APIs can use option key presence; Rust can use `Option<u64>`.
- Reviewer rounds are useful for cross-language semantic drift. The first review round found the explicit-zero monotonic divergence before closure.

## Followup

No follow-up SOWs created from this work.

## Regression Log

None yet.
