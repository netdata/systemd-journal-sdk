# SOW-0029 - Compression Threshold Parity

## Status

Status: open

Sub-state: Split from SOW-0022 Gap 5. Needs user decision before implementation because changing defaults is a public behavior change.

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

- Whether the user wants SDK default thresholds changed to systemd's default of 512, or wants the existing SDK default kept and explicitly documented.

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

Status: needs-user-decision

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

- Pending user decision.

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

Pending.

## Validation

Pending.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.
