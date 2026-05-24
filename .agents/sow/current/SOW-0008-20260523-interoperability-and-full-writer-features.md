# SOW-0008 - Interoperability And Full Writer Features

## Status

Status: in-progress

Sub-state: active after Go, Rust, Node.js, and Python baseline SDK/journalctl slices completed.

## Requirements

### Purpose

Complete cross-language interoperability and close remaining writer feature gaps, including compression and Forward Secure Sealing where in scope.

### Assistant Understanding

Facts:

- This phase requires all baseline language SDKs to pass shared conformance first.
- The Go baseline is split across SOW-0005 (writer first) and SOW-0010 (reader and journalctl completion).
- It closes cross-language interoperability and remaining writer feature gaps.

Inferences:

- Compression and Forward Secure Sealing decisions should be based on the completed baseline feature matrix.

Unknowns:

- Exact compression-writing and FSS implementation depth remains to be determined from the baseline feature matrix and systemd reference evidence. If a safe production-grade implementation would make this SOW too broad, the work must split concrete follow-up SOWs before close.

### Acceptance Criteria

- Every writer/reader pair in Rust, Go, Node.js, and Python passes the interoperability matrix.
- Every writer passes live stock `journalctl --file` and stock libsystemd reader tests while appending.
- Every reader passes live-read tests against every repository writer while appending, plus stock systemd writer evidence where the environment can provide it without violating repository-boundary rules.
- Writer feature gaps from earlier phases are either implemented or represented by concrete follow-up SOWs.
- Compression writing is tested across languages where implemented.
- Forward Secure Sealing support is implemented or explicitly split into a narrower follow-up with evidence.
- No changes are made outside this repository.

## Analysis

Sources checked:

- Product scope spec.
- Pending language SOWs.

Current state:

- SOW-0004, SOW-0005, SOW-0006, SOW-0007, SOW-0010, SOW-0011, SOW-0012, and SOW-0013 are complete.
- Baseline language SDKs and file-backed journalctl slices exist for Go, Rust, Node.js, and Python.
- Each current writer feature slice has passed stock-reader live compatibility for its claimed writer surface.

Risks:

- FSS and compression are high-risk due to crypto, compression, and verification semantics.
- Cross-language subtle differences can corrupt files or hide reader bugs.
- Live concurrency differences can make closed-file verification pass while stock readers fail during normal one-writer/multiple-reader operation.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Full compatibility now requires proving that the four pure-language SDKs can operate on the same journal files, including closed-file interoperability, live one-writer/multiple-reader behavior across repository writers/readers, and explicit tracking of remaining writer feature gaps. The root risk is no longer missing language baselines; it is cross-language mismatch in file layout, match/cursor semantics, binary fields, directory ordering, rotation/retention behavior, compression handling, and active-writer publication windows.

Evidence reviewed:

- Product scope spec.
- Completed language and compatibility SOWs:
  - `.agents/sow/done/SOW-0004-20260523-rust-sdk-and-journalctl.md`
  - `.agents/sow/done/SOW-0005-20260523-go-sdk-and-journalctl.md`
  - `.agents/sow/done/SOW-0006-20260523-node-sdk-and-journalctl.md`
  - `.agents/sow/done/SOW-0007-20260523-python-sdk-and-journalctl.md`
  - `.agents/sow/done/SOW-0010-20260523-go-reader-and-journalctl-completion.md`
  - `.agents/sow/done/SOW-0011-20260523-live-concurrency-compatibility-gate.md`
  - `.agents/sow/done/SOW-0012-20260523-go-writer-binary-fields.md`
  - `.agents/sow/done/SOW-0013-20260523-go-directory-writer-rotation-retention.md`
- Shared contracts under `tests/conformance/` and `tests/conformance/live/`.
- Current SDK directories: `go/`, `rust/`, `node/`, and `python/`.

Affected contracts and surfaces:

- Writer file format features.
- Cross-language fixture matrix.
- Live cross-language reader behavior.
- Directory writer rotation/retention semantics.
- File-backed journalctl compatibility evidence.
- Verification behavior.
- Documentation.

Existing patterns to reuse:

- Shared conformance harness.
- Language SDK contracts.
- Per-language livewriter commands.
- Stock-reader live concurrency harness.
- Language adapters and file-backed journalctl commands.
- Product-scope feature-slice documentation style from completed SOWs.

Risk and blast radius:

- FSS and compression are high-risk due to crypto, compression, and verification semantics.
- Cross-language subtle differences can corrupt files or hide reader bugs.
- Live matrix tests can expose race windows that closed-file tests miss.
- Directory writer tests can remove or archive files if retention filters are wrong; all generated matrix files must stay inside `.local/`.

Sensitive data handling plan:

- No sensitive runtime data expected. Matrix fixtures must use synthetic fields and generated files under `.local/`; durable artifacts must record only sanitized paths, commands, counts, versions, and verdicts.

Implementation plan:

1. Build a committed or documented matrix runner that generates journal files from each language writer and reads them with every language reader plus stock `journalctl` where applicable.
2. Build or extend live matrix coverage so each repository reader consumes live files produced by each repository writer, reusing `tests/conformance/live/` and per-language `livewriter` commands.
3. Run the matrix, record exact commands, stock systemd version, entry counts, reader counts, failures, and transient retry rules.
4. Fix interoperability bugs found by the matrix without widening language-specific APIs unless specs/SOW are updated.
5. Inventory remaining writer feature gaps: compressed DATA object writing, xz/lz4/zstd parity, compact journal support, verification/FSS support, and directory ordering limitations.
6. Implement safe scoped writer features in this SOW where practical; split any high-risk compression/FSS/compact work into concrete follow-up SOWs with evidence if implementation would exceed the current SOW's safe blast radius.
7. Update specs, docs, SOW-status, and follow-up mapping before close.

Validation plan:

- Closed-file writer/reader matrix passes for Go, Rust, Node.js, and Python writers/readers.
- Stock `journalctl --verify --file` and stock reader checks pass for generated writer files where the claimed writer feature slice supports verification.
- Live stock-reader and cross-language concurrency matrix passes for every current repository writer/reader pair.
- systemd-compatible verification evidence is recorded where applicable.
- Dependency audit remains clean.
- `.agents/sow/audit.sh` and `git diff --check` pass before close.

Artifact impact plan:

- Specs: update writer feature reality.
- End-user/operator docs: update feature support matrix.
- Runtime project skills: update if new compatibility workflow is durable.
- SOW lifecycle: active in `current/` during implementation, then close to `done/` with implementation and SOW lifecycle changes in one commit.
- SOW-status.md: update when this SOW moves to current or closes.

Open decisions:

- No immediate user decision blocks activation. The implementation may split compression-writing, compact journal, verification, and FSS work into narrower follow-up SOWs if evidence shows they are not safe to complete together with the interoperability matrix.

## Implications And Decisions

1. Interoperability and full writer completion boundary
   - Current state: SOW-0004, SOW-0005, SOW-0006, SOW-0007, and SOW-0010 are complete and pass their shared conformance gates.
   - Required before implementation: record the completed baseline feature matrix and decide from evidence whether any remaining compression or Forward Secure Sealing work needs narrower follow-up SOWs.
   - Implication: this SOW closes cross-language file compatibility after all baseline SDKs exist.
   - Risk: starting before all language baselines pass can hide whether failures come from core format handling, individual SDK bugs, or interoperability assumptions.

## Plan

1. Move this SOW to `current/` after SOW-0007 closeout commit.
2. Record the completed feature matrix and remaining writer gaps before writer-feature implementation.
3. Delegate interoperability and writer-feature work using the repository-boundary block.
4. Review matrix results, systemd-compatible evidence, dependency audit, docs, and SOW audit before closing.

## Delegation Plan

- Implementer: `llm-netdata-cloud/minimax-m2.7-coder`, or fallback to `llm-netdata-cloud/qwen3.6-plus` then `llm-netdata-cloud/glm-5.1` if minimax fails or is unavailable.
- Reviewers: at least two from `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`, `llm-netdata-cloud/glm-5.1`.
- Every prompt must include the canonical repository-boundary block from `AGENTS.md`.
- Failure handling: record implementer or reviewer model failure in this SOW, substitute only from the approved model list, rerun full-scope review after fixes, and do not close if `.agents/sow/audit.sh` fails.

## Execution Log

- 2026-05-24: Activated after Python closeout commit `b1276a0`, with all baseline language SDK/journalctl slices completed.

## Validation

Activation evidence:

- Passed: SOW-0007 closeout commit `b1276a0` exists before activation.
- Passed: `.agents/sow/audit.sh` was run after moving this SOW to `current/`; status/directory consistency passed and only this activation SOW is current.

Acceptance criteria evidence:

- Implementation validation has not started yet. The active gate defines the closed-file matrix, live cross-language matrix, stock-reader compatibility checks, writer-gap inventory, dependency audit, docs/spec updates, external reviews, and final SOW audit required before close.

Tests or equivalent validation:

- Activation audit passed. Matrix implementation and test evidence will be added during implementation.

Real-use evidence:

- No new runtime matrix has been generated during activation. Real-use evidence will be collected from generated `.local/` matrix files and stock systemd tools during implementation.

Reviewer findings:

- No implementation review has run for this SOW yet. Reviewer prompts must cover the full SOW scope and active matrix evidence after implementation.

Same-failure scan:

- Activation reviewed completed language SOWs and product-scope limitations for overlap. Implementation must search same-failure classes across all four language SDKs before close.

Sensitive data gate:

- Activation introduced no runtime journal data and no secrets. Planned matrix data must use synthetic fields and generated files under `.local/`; durable artifacts must record only sanitized commands, counts, versions, and verdicts.

Artifact maintenance gate:

- AGENTS.md: no update needed for activation; existing SOW and repository-boundary rules apply.
- Runtime project skills: no update needed for activation; implementation may update compatibility/orchestration skills if the matrix creates a durable workflow.
- Specs: no new shipped behavior during activation; specs will update when matrix results or writer feature reality changes.
- End-user/operator docs: no update needed for activation; docs will update if the support matrix changes.
- End-user/operator skills: no output/reference skill is produced during activation.
- SOW lifecycle: moved from `pending/` to `current/` with `Status: in-progress`.
- SOW-status.md: updated for SOW-0008 activation.

Specs update:

- No spec update needed for activation beyond existing product scope.

Project skills update:

- No project skill update needed for activation.

End-user/operator docs update:

- No end-user/operator docs update needed for activation.

End-user/operator skills update:

- No end-user/operator skill update needed for activation.

## Outcome

Pending.

## Lessons Extracted

Pending activation.

## Followup

Pending activation.

## Regression Log

None yet.
