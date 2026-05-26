# SOW-0029 - Compression Threshold Parity

## Status

Status: completed

Sub-state: Implemented, validated, reviewed, and ready for commit.

## Requirements

### Purpose

Ensure compression threshold behavior is compatible across Rust, Go, Node.js, Python, and systemd, without surprising SDK users or weakening Netdata performance expectations.

### User Request

Resolve the compatibility gap where SDK writer defaults use a lower compression threshold than systemd, while preserving pure SDK behavior and maintainability.

### Assistant Understanding

Facts:

- systemd v260.1 default compression threshold is 512 bytes and minimum threshold is 8 bytes.
- Rust, Go, Node.js, and Python currently default to 64 bytes.
- Current compression tests use a low threshold intentionally for coverage.

Inferences:

- SDK writers can compress DATA objects that systemd would leave uncompressed by default.
- This is valid journal format behavior but not exact systemd default behavior.

Unknowns:

- None. The user selected systemd behavior.

### Acceptance Criteria

- A user decision is recorded before implementation.
- All four languages apply the same default and minimum threshold policy.
- Tests cover default threshold, minimum threshold clamp, and exact boundary behavior.
- Compression matrices continue to pass for zstd, xz, and lz4 where supported.

## Analysis

Sources checked:

- `SOW-0022-20260525-compatibility-test-gap-audit.md`
- `go/journal/format.go`
- `rust/src/crates/journal-core/src/file/writer.rs`
- `rust/src/crates/journal-log-writer/src/log/config.rs`
- `node/src/lib/writer.js`
- `python/journal/writer.py`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`

Current state:

- Go `defaultCompressThreshold` is 64.
- Rust core and high-level log config default threshold is 64.
- Node.js `DEFAULT_COMPRESS_THRESHOLD` is 64.
- Python `DEFAULT_COMPRESS_THRESHOLD` is 64.
- Current tests prioritize exercising compression paths, not systemd threshold parity.

Risks:

- Changing defaults can affect file size, CPU cost, and benchmark results.
- Keeping defaults can preserve current SDK behavior but leaves systemd default parity weaker.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The SDKs use a lower default compression threshold than systemd. This can increase compression work and produce different compressed/uncompressed object choices from stock systemd for the same input when callers do not configure a threshold.

Evidence reviewed:

- `go/journal/format.go:81`
- `rust/src/crates/journal-core/src/file/writer.rs:26`
- `rust/src/crates/journal-log-writer/src/log/config.rs:136-138`
- `node/src/lib/writer.js:36`
- `python/journal/writer.py:39`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `src/libsystemd/sd-journal/journal-file.c:51-52`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `src/libsystemd/sd-journal/journal-file.c:4127-4133`

Affected contracts and surfaces:

- Writer defaults in all four languages.
- Compression tests and interoperability matrices.
- Benchmarks and Netdata integration guidance.
- Public SDK docs/API examples.

Existing patterns to reuse:

- Existing compression matrix and low-threshold test options.
- Existing writer option surfaces for configured compression threshold.

Risk and blast radius:

- Medium public behavior risk because defaults change generated files and CPU usage.
- Low format risk because both thresholds can produce valid journal files when implemented correctly.

Sensitive data handling plan:

- Use synthetic entries only.

Implementation plan:

1. Record the user decision.
2. Add threshold boundary tests matching the chosen policy.
3. Apply the policy consistently in Rust, Go, Node.js, and Python.
4. Keep explicit low-threshold knobs in tests so compression paths remain covered.
5. Update docs/specs if defaults change or are intentionally documented as SDK-specific.

Validation plan:

- Run per-language writer tests.
- Run compression matrix for all algorithms.
- Run stock `journalctl --verify --file` on generated threshold-boundary files.

Artifact impact plan:

- AGENTS.md: no expected update.
- Runtime project skills: no expected update unless threshold parity becomes a mandatory validation rule.
- Specs: update `product-scope.md` for the accepted default/minimum policy.
- End-user/operator docs: update public API docs if defaults or examples change.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: status remains open until the user decision is recorded.
- SOW-status.md: update on activation and completion.

Open-source reference evidence:

- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
  - `src/libsystemd/sd-journal/journal-file.c`
  - `src/libsystemd/sd-journal/test-journal.c`

Open decisions:

1. Compression threshold default:
   - Option A: Change SDK defaults to systemd parity: default 512 bytes, clamp configured values below 8 to 8.
     - Pros: strongest default compatibility with stock systemd.
     - Cons: may reduce compression coverage in default SDK use and alter benchmark/file-size behavior.
     - Risk: users expecting current 64-byte default see different output.
   - Option B: Keep SDK default 64, clamp minimum to 8, and document the default as intentionally SDK-specific.
     - Pros: preserves current behavior and likely current compression coverage.
     - Cons: weaker systemd default parity.
     - Risk: default-generated files differ more from systemd.
   - Recommendation: Option A, because the project prioritizes systemd compatibility unless the user chooses a Netdata-specific performance/file-size default.

## Implications And Decisions

- User decision on 2026-05-26: "do whatever systemd does."
- Accepted policy:
  - Default compression threshold: `512` bytes.
  - Minimum configured compression threshold: `8` bytes.
  - Configured thresholds below `8` must be clamped to `8`.
- Implications:
  - This intentionally changes the SDK default from the current `64` bytes to systemd parity.
  - Compression-path tests must keep explicit low-threshold settings where needed so coverage remains high.

## Plan

1. Record the user decision.
2. Add boundary tests.
3. Implement policy consistently in all languages.
4. Validate and review as one batch.

## Delegation Plan

Implementer:

- Local implementation by the project manager, per current routing decision.

Reviewers:

- Read-only reviewers from the active pool after implementation: minimax, kimi, qwen, glm. Mimo is skipped.

Failure handling:

- Record failures before changing the threshold policy or scope.

## Execution Log

- 2026-05-26: Activated SOW after user selected systemd compression threshold behavior for SOW-0029 and Netdata monotonic behavior for SOW-0030.
- 2026-05-26: Implemented the systemd compression threshold policy across Rust, Go, Node.js, and Python.
- 2026-05-26: Updated SDK docs/specs for the accepted threshold behavior.
- 2026-05-26: Go zero-value `Options{}` keeps `CompressThresholdBytes == 0` as the unset/default marker so the zero-value API uses the systemd 512-byte default. Positive Go thresholds below 8 clamp to 8. Rust, Node.js, and Python can distinguish explicit low values and clamp them to 8.
- 2026-05-26: Added per-language threshold tests for default-below, default-exact, minimum-clamp-below, and minimum-clamp-eligible payload behavior.
- 2026-05-26: Kept `run_compact_matrix.py` as an explicit low-threshold compression coverage harness and documented why it uses `16` instead of the SDK default.
- 2026-05-26: Updated the interoperability README example to use the valid minimum threshold `8`.
- 2026-05-26: Reused the shared Rust threshold normalizer in both Rust writer layers.

## Validation

- `go test ./...` from `go/`: passed.
- `cargo test` from `rust/`: passed.
- `node test/all.js` from `node/`: passed.
- `.local/python-venv/bin/python python/test_all.py`: passed.
- `python3 python/test_all.py`: failed because the system interpreter does not have `lz4`; rerun with the project venv passed.
- `PATH=/home/costa/Documents/systemd-journal-sdk/.local/python-venv/bin:$PATH .local/python-venv/bin/python tests/interoperability/run_compression_matrix.py --compression zstd xz lz4`: passed 216/216 on systemd 260 (260.1-2-manjaro), latest results written under `.local/interoperability/compression-matrix-results-20260526-223551.json`.
- `rg -n "DEFAULT_COMPRESS_THRESHOLD\s*=\s*64|defaultCompressThreshold\s*=\s*64|compress_threshold:\s*64|compressionThresholdBytes:\s*64|compression_threshold_bytes.*64|compress-threshold.*64|default_value_t\s*=\s*64|default=64" go rust node python tests/interoperability`: no stale default-threshold matches.

Acceptance criteria evidence:

- User decision recorded in `## Implications And Decisions`.
- All four languages now default to `512` and clamp configured values below `8` to `8`.
- Per-language tests cover default threshold, minimum clamp, and exact/default boundary behavior.
- Compression interoperability matrix passed for zstd, xz, and lz4.

Reviewer evidence:

- Round 1 reviewers: minimax, kimi, qwen, and glm all returned `PRODUCTION GRADE` with low-severity notes.
- Round 1 findings handled:
  - Added clamp+eligible-payload tests in all four languages.
  - Added the `run_compact_matrix.py` low-threshold coverage comment.
  - Updated the compact-matrix README example from threshold `1` to `8`.
  - Clarified Go zero-value and non-zero clamp documentation.
  - Reused Rust `normalize_compress_threshold()` in `journal-log-writer`.
- Round 2 reviewers:
  - minimax: `PRODUCTION GRADE`. Its Node.js journalctl availability finding was a false positive; `verifyJournalFileIfAvailable()` probes `journalctl --version` before verify.
  - glm: `PRODUCTION GRADE`. It flagged the pre-existing Node.js XZ explicit 80-byte guard difference as non-blocking and outside this SOW; runtime behavior remains covered by existing Node.js xz support and this SOW did not change algorithm-specific XZ behavior.
  - qwen: `PRODUCTION GRADE`. It confirmed the systemd threshold evidence, API behavior, and SOW completeness.
  - kimi: `PRODUCTION GRADE` before the final Rust test parity assertion; it flagged a low Rust sub-minimum clamp test gap. The gap was fixed and `cargo test` passed again. The follow-up run ended without a visible final verdict after producing validation output, so it is recorded as inconclusive after the low finding was implemented and validated.

Same-failure search:

- Stale `64` default-threshold patterns were searched across Rust, Go, Node.js, Python, and interoperability tests; no relevant stale SDK default was found.
- Remaining `64` hits found by reviewers were unrelated compression-library documentation, 64-bit values, file modes, or decompression reserve constants.

Sensitive data gate:

- Only synthetic test payloads, source paths, and command outputs were recorded. No raw sensitive data was added.

Artifact maintenance gate:

- `AGENTS.md`: no update needed; workflow and policy unchanged.
- Runtime project skills: no update needed; existing journal compatibility skill already covers compression-matrix validation.
- Specs: `.agents/sow/specs/product-scope.md` updated with the shared systemd threshold policy.
- End-user/operator docs: Rust, Go, Node.js, Python READMEs and Go package docs updated.
- End-user/operator skills: no output/reference skills exist for this repository.
- SOW lifecycle: this file moves from `current/` to `done/` with `Status: completed`.
- `SOW-status.md`: updated to remove SOW-0029 from Current and add it to Done.

## Outcome

Completed.

Rust, Go, Node.js, and Python writers now follow the systemd compression threshold policy for default behavior:

- default DATA compression threshold: `512` bytes;
- minimum configured threshold: `8` bytes;
- compression eligibility starts at `payload_len >= threshold`;
- Go zero-value `Options{}` continues to mean "unset/default", so `CompressThresholdBytes == 0` uses `512`.

Explicit low thresholds remain available in test harnesses where compression-path coverage is the goal, including compact matrix fixtures.

## Lessons Extracted

- Compression threshold tests must use payload lengths that include the full DATA payload (`FIELD=value`), not only the field value bytes.
- Very small eligible payloads may still be stored uncompressed when the compressed output is larger; tests should distinguish threshold eligibility from final compressed-object selection.
- Harness defaults are not always SDK defaults. If a harness intentionally uses a low threshold for coverage, the reason needs to be written next to the option.

## Followup

No follow-up SOW is required from this SOW.

Reviewer observations that are intentionally not converted into follow-up work:

- Node.js lacks an explicit XZ `>=80` guard in one path, but this is pre-existing, outside compression threshold default parity, and existing Node.js xz validation covers runtime behavior.
- Rust, Node.js, and Python explicit `0` threshold behavior differs from Go zero-value behavior by design. The spec records that Go's zero value is an unset/default marker.
- Negative signed thresholds in Go, Node.js, and Python clamp to `8`, producing valid files and matching the "below 8 clamps to 8" policy.

## Regression Log

None yet.
