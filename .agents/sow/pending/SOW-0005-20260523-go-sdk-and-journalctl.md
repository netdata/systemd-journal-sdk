# SOW-0005 - Go Writer First

## Status

Status: open

Sub-state: next implementation SOW after SOW-0003 completes. User priority requires the Go writer before Rust, Go reader/journalctl completion, Node.js, Python, interoperability, or benchmarks.

## Requirements

### Purpose

Deliver a production-grade pure-Go systemd journal writer first, for Netdata plugin integration use.

This SOW intentionally narrows the previous Go SDK scope to the writer. The Go reader facade and Go journalctl rewrite remain required, but are deferred to a follow-up SOW after the writer is usable.

### User Request

The user needs the Go writer finished before anything else because it is needed in a Netdata plugin. The order is Go writer first, then everything else.

### Assistant Understanding

Facts:

- Go writer must be the first implementation deliverable after the shared harness is accepted.
- Go must use no CGO and no system journal library linkage.
- The writer must produce systemd journal files that readers in this repo and systemd-compatible tooling can read according to the project compatibility target.
- Rust, Go reader/journalctl completion, Node.js, Python, interoperability, benchmarks, and optimization remain required but are lower priority until this writer is done.

Inferences:

- The writer SOW should be smaller than the original full Go SDK SOW so the Netdata plugin use case is not delayed by Go reader/journalctl completion.
- Minimal read-back tooling may be needed only to validate writer output; it should not expand this SOW into the full Go reader implementation.

Unknowns:

- Exact initial writer feature subset must be enriched from SOW-0003 and systemd journal file format evidence before activation.

### Acceptance Criteria

- Go exposes an idiomatic writer API that can create and append systemd journal entries to journal files.
- Go writer uses no CGO and no system journal library linkage.
- Go writer produces journal files readable by the repo's imported Rust reader or other shared validation tooling available at activation time.
- Go writer output is validated against the shared writer/file-format conformance cases available after SOW-0003.
- Go writer implements systemd journal file locking/concurrency expectations for one writer and multiple readers, or records any initial scoped limitation with follow-up SOW coverage.
- Go writer has focused docs/examples sufficient for Netdata plugin integration.
- Go reader facade, Go journalctl rewrite, and full cross-language matrix are explicitly deferred and tracked after this SOW.
- No changes are made outside this repository.

## Analysis

Sources checked:

- Product scope spec.
- Pending harness SOW.
- User priority update on 2026-05-23.

Current state:

- Blocked until SOW-0003 completes and is committed.
- This SOW now precedes Rust SDK/journalctl, Go reader/journalctl completion, Node.js, Python, interoperability, and benchmarks.

Risks:

- CGO or native dependency leakage would violate the project goal.
- Incorrect journal object layout, hash tables, tag objects, entry arrays, or file header fields can produce unreadable or corrupt files.
- File locking mistakes can break the one-writer/multiple-reader journal rule and the Netdata plugin use case.
- Over-expanding into reader/journalctl work would delay the user-prioritized writer deliverable.

## Pre-Implementation Gate

Status: blocked

Problem / root-cause model:

- Go writer implementation must follow the systemd journal file format and the shared harness contract established by SOW-0003.
- The immediate product need is writer output for a Netdata plugin, not a full Go SDK/journalctl stack.

Evidence reviewed:

- Product scope spec.
- User priority update on 2026-05-23.
- Pending harness SOW.

Affected contracts and surfaces:

- Go writer public API.
- Journal file format writer behavior.
- File locking and concurrency behavior.
- Shared harness writer adapter.
- Dependency policy.

Existing patterns to reuse:

- Imported Rust writer/reference behavior from SOW-0002.
- Shared fixtures and conformance harness from SOW-0003.
- systemd journal file format evidence.

Risk and blast radius:

- CGO or native dependency leakage would violate the project goal.
- Writer behavior must remain interoperable with Rust and future languages.
- Incorrect write ordering, object offsets, hashes, compression framing, or file state transitions can produce journals that appear to write successfully but fail under readers.
- Go-specific risks, such as pure-Go binary serialization, file mapping strategy, append safety, fsync behavior, and locking behavior, must be enriched before this SOW moves to current.

Sensitive data handling plan:

- No sensitive runtime data expected.

Implementation plan:

1. Enrich the writer feature subset from SOW-0003 and systemd journal format evidence.
2. Design the idiomatic Go writer API and file lifecycle.
3. Implement pure-Go journal file creation and append path.
4. Implement required file locking and flush/sync behavior.
5. Wire writer-focused shared tests and read-back validation.
6. Add Netdata-plugin-oriented docs/examples for writer usage.

Validation plan:

- Writer-focused shared conformance cases pass Go.
- Go package tests pass.
- Dependency audit confirms no CGO.
- Output journal files are read back by available repository readers or systemd-compatible file-backed tooling.
- Corruption/partial-write behavior is tested where SOW-0003 provides relevant cases.

Artifact impact plan:

- Specs: update writer feature contract and Go writer priority.
- End-user/operator docs: create Go writer docs/examples.
- SOW lifecycle: blocked until SOW-0003 completes.
- SOW-status.md: update when this SOW moves to current or closes.

Open decisions:

- No user decision is currently needed. The priority decision is recorded: Go writer first.

## Implications And Decisions

1. Go writer-first priority
   - Current state: resolved by user decision on 2026-05-23.
   - Selection: deliver the pure-Go journal writer before Rust, Go reader/journalctl completion, Node.js, Python, interoperability, or benchmarks.
   - Rationale: the writer is needed for a Netdata plugin integration.
   - Implication: this SOW is intentionally narrowed to writer delivery and validation.
   - Risk: deferring Go reader/journalctl means the Go SDK is not complete after this SOW; follow-up SOW coverage is required.

2. Go no-CGO writer strategy
   - Current state: blocked on SOW-0003 completion and writer feature enrichment.
   - Required before activation: record how systemd journal file creation, append, file I/O, locking, sync, and dependency constraints map to idiomatic Go.
   - Implication: the Go writer must pass writer-focused conformance without CGO or native journal linkage.
   - Risk: incorrect binary serialization or locking assumptions can corrupt files even if local Go tests pass.

## Plan

1. Wait for SOW-0003 to complete and commit.
2. Enrich writer-specific risks, systemd journal format requirements, and Go API mapping before activation.
3. Delegate Go writer implementation using the repository-boundary block.
4. Review writer conformance, read-back validation, dependency audit, docs/examples, and audit output before closing.

## Delegation Plan

- Implementer: `llm-netdata-cloud/minimax-m2.7-coder`, or fallback to `llm-netdata-cloud/qwen3.6-plus` then `llm-netdata-cloud/glm-5.1` if minimax fails or is unavailable.
- Reviewers: at least two from `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`, `llm-netdata-cloud/glm-5.1`.
- Every prompt must include the canonical repository-boundary block from `AGENTS.md`.
- Failure handling: record implementer or reviewer model failure in this SOW, substitute only from the approved model list, rerun full-scope review after fixes, and do not close if `.agents/sow/audit.sh` fails.

## Execution Log

- 2026-05-23: Scope changed from full Go SDK/journalctl to Go writer-first per user priority. Go reader/journalctl completion moved to follow-up planning.

## Validation

Pending activation and implementation.

## Outcome

Pending.

## Lessons Extracted

- Go reader facade and Go journalctl rewrite remain required after the writer-first SOW.
- Full cross-language writer/reader matrix remains required after all language implementations exist.

## Followup

Pending activation.

## Regression Log

None yet.
