# SOW-0021 - Node.js XZ DATA Compression

## Status

Status: open

Sub-state: pending after SOW-0017 split.

## Requirements

### Purpose

Complete the remaining Node.js XZ-compressed DATA object support while preserving stock systemd journal compatibility and the repository's no-systemd-journal-library boundary.

### User Request

The user requires all writer implementations to support systemd-defined journal compression where applicable and to use current common compression libraries rather than manually implementing compression algorithms when suitable libraries exist.

### Assistant Understanding

Facts:

- SOW-0017 implements Rust/Go XZ+LZ4, Python XZ+LZ4, and Node.js LZ4.
- Node.js XZ remains unsupported in SOW-0017.
- The Node.js writer API is currently synchronous.
- `node-liblzma@5.0.1` was the latest checked npm XZ/LZMA2 candidate during SOW-0017 Phase 2B.
- `node-liblzma@5.0.1` default package metadata includes native addon dependencies; its non-native WASM API is async.

Inferences:

- Node.js XZ likely requires either a small synchronous API extension, an async compression path that does not disrupt existing writer calls, or a different current package that provides synchronous `.xz`/LZMA2 `CHECK_NONE` output without native addons.

Unknowns:

- Whether a current Node.js compression package can synchronously write systemd-compatible `.xz` streams without native addons.
- Whether the user will accept a Node.js async writer API extension specifically for XZ.

### Acceptance Criteria

- Node.js reader can read XZ-compressed DATA objects written by Rust, Go, Python, stock-compatible fixtures, and Node.js if writing is implemented.
- Node.js writer can write XZ-compressed DATA objects, or the SOW records a user decision accepting an API/policy limitation.
- Written Node.js XZ journals pass stock `journalctl --verify --file`, stock journalctl JSON/export reads, stock libsystemd reads, and repository Rust/Go/Python/Node readers where supported.
- Dependency review records latest stable package versions and why the selected path is acceptable.

## Analysis

Sources checked:

- `.agents/sow/current/SOW-0017-20260524-xz-lz4-data-writing.md`
- `node/src/lib/writer.js`
- `node/src/lib/entry.js`
- npm metadata for `node-liblzma@5.0.1`

Current state:

- Node.js LZ4 DATA object reading/writing is implemented in SOW-0017.
- Node.js XZ DATA objects are still rejected.

Risks:

- Converting the writer API to async could affect all Node.js writer callers.
- Native addon usage would violate the current Node.js project constraint unless the user explicitly changes it.
- WASM packaging may add runtime initialization and deployment complexity.

## Pre-Implementation Gate

Status: needs-user-decision

Problem / root-cause model:

- systemd XZ DATA objects require `.xz` streams using LZMA2 and `CHECK_NONE`.
- The current Node.js writer path is synchronous, but the checked non-native package path for XZ is async WASM.

Evidence reviewed:

- `node/src/lib/writer.js`: synchronous `Writer.create()` and `append()` API.
- `node/src/lib/entry.js`: XZ DATA objects remain unsupported after SOW-0017 Phase 2B.
- `node-liblzma@5.0.1` npm metadata: default package includes native addon dependencies; WASM path is non-native but async.

Affected contracts and surfaces:

- Node.js writer API.
- Node.js reader DATA decompression.
- Node.js package dependencies and packaging.
- Shared compression matrix.
- README and product-scope specs.

Existing patterns to reuse:

- SOW-0017 Node.js LZ4 helper style in `node/src/lib/lz4-block.js`.
- Shared `tests/interoperability/run_compression_matrix.py`.
- Existing synchronous writer option parsing.

Risk and blast radius:

- Medium. Reader support is isolated, but writer support may require API or package-policy decisions.

Sensitive data handling plan:

- Use synthetic compression fixtures only. Record package metadata and validation commands; no secrets or customer data.

Implementation plan:

1. Re-check latest Node.js XZ/LZMA2 package options and licenses.
2. Present a user decision if support requires async writer API changes or native addon policy changes.
3. Implement Node.js XZ reader/writer support after the decision.
4. Extend compression matrix coverage for Node.js XZ.

Validation plan:

- Node package tests.
- Node.js XZ compression matrix against stock journalctl, stock libsystemd, Rust, Go, Python, and Node readers.
- zstd/lz4 regression matrices remain passing.
- External reviewer pass.

Artifact impact plan:

- AGENTS.md: no update expected unless dependency/native-addon policy changes.
- Runtime project skills: update only if Node.js XZ adds a durable workflow.
- Specs: update Node.js reader/writer support slice.
- End-user/operator docs: update Node README and interoperability README.
- End-user/operator skills: none expected.
- SOW lifecycle: follow-up split from SOW-0017.
- SOW-status.md: add pending entry.

Open-source reference evidence:

- No external source repository was checked yet. Package metadata was enough to identify the current decision point.

Open decisions:

- Decide whether Node.js XZ may add an async writer path, use a native addon compression package, or remain unsupported until a synchronous non-native package is found.

## Implications And Decisions

Pending user decision.

## Plan

1. Dependency/API feasibility review.
2. User decision if required.
3. Implementation and shared matrix validation.
4. Review and commit.

## Delegation Plan

Implementer:

- Preferred implementer is `llm-netdata-cloud/kimi-k2.6`.

Reviewers:

- `llm-netdata-cloud/minimax-m2.7-coder`, `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`, and `llm-netdata-cloud/glm-5.1`.

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

- Record implementer failure, reviewer failure, audit failure, dependency rejection, or model unavailability before changing plan or model.

## Execution Log

### 2026-05-24

- Created as a follow-up split from SOW-0017 to track Node.js XZ DATA object support.

## Validation

Pending implementation.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.
