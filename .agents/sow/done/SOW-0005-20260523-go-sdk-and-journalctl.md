# SOW-0005 - Go Writer First

## Status

Status: completed

Sub-state: completed after implementation, validation, external review, and audit. This SOW delivered the Go writer before Rust, Go reader/journalctl completion, Node.js, Python, interoperability, or benchmarks.

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

- No activation-blocking unknowns remain.
- Broader writer features, including compression, Forward Secure Sealing, compact format, and arbitrary historical-file append support, remain tracked for later SOWs.

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

- SOW-0003 completed and committed as `7e99385`.
- This SOW now precedes Rust SDK/journalctl, Go reader/journalctl completion, Node.js, Python, interoperability, and benchmarks.

Risks:

- CGO or native dependency leakage would violate the project goal.
- Incorrect journal object layout, hash tables, tag objects, entry arrays, or file header fields can produce unreadable or corrupt files.
- File locking mistakes can break the one-writer/multiple-reader journal rule and the Netdata plugin use case.
- Over-expanding into reader/journalctl work would delay the user-prioritized writer deliverable.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Go writer implementation must follow the systemd journal file format and the shared harness contract established by SOW-0003.
- The immediate product need is writer output for a Netdata plugin, not a full Go SDK/journalctl stack.

Evidence reviewed:

- Product scope spec.
- User priority update on 2026-05-23.
- SOW-0003 completed conformance harness.
- Imported Rust writer: `rust/src/crates/jf/journal_file/src/writer.rs`.
- Imported Rust file/header/object definitions: `rust/src/crates/jf/journal_file/src/file.rs`, `rust/src/crates/jf/journal_file/src/object.rs`, and `rust/src/crates/jf/journal_file/src/hash.rs`.

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

Initial feature slice:

- Regular, non-compact journal files.
- Uncompressed DATA objects.
- Keyed hash tables using the journal file ID.
- File creation, close, and reopen/append for files created by this Go writer.
- Data and field object de-duplication through hash-table lookup.
- Entry arrays and per-DATA entry links sufficient for reader filtering.
- Linux one-writer locking with shared-reader compatibility.

Explicitly deferred:

- DATA compression and writer-side compression selection.
- Forward Secure Sealing and TAG objects.
- Compact format writer support.
- Appending to arbitrary historical/systemd-created journal variants.
- Full Go reader facade and Go journalctl rewrite; tracked by SOW-0010.

Risk and blast radius:

- CGO or native dependency leakage would violate the project goal.
- Writer behavior must remain interoperable with Rust and future languages.
- Incorrect write ordering, object offsets, hashes, compression framing, or file state transitions can produce journals that appear to write successfully but fail under readers.
- Go-specific risks, such as pure-Go binary serialization, file mapping strategy, append safety, fsync behavior, and locking behavior, must be enriched before this SOW moves to current.

Sensitive data handling plan:

- No sensitive runtime data expected.
- Test journal entries use synthetic values only.
- Generated journal files remain in test temporary directories and are not committed.

Sensitive data gate:

- Before review and close, scan changed durable artifacts for raw secrets, credentials, bearer tokens, SNMP communities, personal data, non-private customer-identifying IPs, private endpoints, and proprietary incident details.
- Do not commit generated journal files or local scratch output.

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
- SOW lifecycle: move this SOW to `current/` during activation.
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
   - Current state: ready for implementation.
   - Required before close: record how systemd journal file creation, append, file I/O, locking, sync, and dependency constraints map to idiomatic Go.
   - Implication: the Go writer must pass writer-focused conformance without CGO or native journal linkage.
   - Risk: incorrect binary serialization or locking assumptions can corrupt files even if local Go tests pass.

## Plan

1. Activate this SOW by moving it to `current/` and setting `Status: in-progress`.
2. Implement the initial pure-Go writer feature slice.
3. Add writer-focused Go tests and systemd-compatible read-back validation where available.
4. Review writer conformance, dependency audit, docs/examples, and audit output before closing.

## Delegation Plan

- Implementer: `llm-netdata-cloud/minimax-m2.7-coder`, or fallback to `llm-netdata-cloud/qwen3.6-plus` then `llm-netdata-cloud/glm-5.1` if minimax fails or is unavailable.
- Reviewers: at least two from `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`, `llm-netdata-cloud/glm-5.1`.
- Every prompt must include the canonical repository-boundary block from `AGENTS.md`.
- Failure handling: record implementer or reviewer model failure in this SOW, substitute only from the approved model list, rerun full-scope review after fixes, and do not close if `.agents/sow/audit.sh` fails.

## Execution Log

- 2026-05-23: Scope changed from full Go SDK/journalctl to Go writer-first per user priority. Go reader/journalctl completion moved to follow-up planning.
- 2026-05-23: Activated after SOW-0003 completed and committed.
- 2026-05-23: Initial writer slice recorded as regular, uncompressed, keyed-hash journal creation plus append for files created by this writer.
- 2026-05-23: Implemented pure-Go writer package with file creation, append, reopen/append, hash-table de-duplication, entry arrays, per-DATA entry links, advisory writer locking, docs, and writer-focused tests.
- 2026-05-23: Review round 1 ran with Minimax, Kimi, Mimo, Qwen, and GLM as read-only reviewers. Minimax, Mimo, Qwen, and GLM returned production-grade verdicts with nonblocking test coverage notes. Kimi returned production-grade wording but included a blocking concrete failure: reopen plus default monotonic timestamps made `journalctl --verify` fail with "Entry timestamp out of synchronization".
- 2026-05-23: Fixed reopen monotonic generation by anchoring `Open()` to the existing tail monotonic timestamp. Added tests for reopen/default monotonic verification, entry-array growth, per-DATA entry-array growth through repeated fields, hash-bucket collision de-duplication, `Sync()`, idempotent `Close()`, append-after-close, leading-digit field rejection, and 64-character field-name limit.
- 2026-05-23: Dispositioned reviewer false positives: per-DATA entry array concern is expected systemd `link_entry_into_array_plus_one` behavior because the first entry remains inline in `data.entry_offset`; xor-hash before entry item de-dup matches systemd `journal_file_append_entry` behavior; hash table offsets intentionally point to hash table items, not the hash table object header, matching systemd.
- 2026-05-23: Review round 2 ran with Minimax, Kimi, Mimo, Qwen, and GLM as read-only reviewers after fixes. Minimax, Mimo, Qwen, and GLM returned `PRODUCTION GRADE` with no blocking findings. Kimi completed the key validation commands successfully but hung before emitting a final verdict; the hung reviewer process was stopped with a targeted PID kill and a process check found no lingering external-agent process.
- 2026-05-23: Dispositioned round-2 nonblocking findings as follow-up coverage: payload size limits, goroutine-safety docs, additional positive field-name boundary tests, binary value tests, ENOSPC/short-write fault injection, and broader writer feature support are not blockers for the accepted writer-first slice.

## Validation

- `go test -count=1 ./...` from `go/`: pass after reopen monotonic fix and added stress tests.
- `CGO_ENABLED=0 go test -count=1 ./...` from `go/`: pass after reopen monotonic fix and added stress tests.
- `go vet ./...` from `go/`: pass.
- `go list -deps -f '{{if .CgoFiles}}{{.ImportPath}} {{.CgoFiles}}{{end}}' ./...` from `go/`: no output; no package in this module reports CGO files.
- `go test -count=1 -run 'Test(OpenAppendDefaultMonotonicPreservesJournalctlVerify|EntryArrayGrowthAndJournalctlReadback|JournalctlReadsCreatedJournal)' -v ./journal` from `go/`: pass; includes reopen/default monotonic, `journalctl --verify --file`, entry-array growth, per-DATA repeated-field growth, and filtered `journalctl --file --output=json` read-back.
- `python3 tests/conformance/runner/manifest_checker.py validate tests/conformance/manifests/conformance-v01.json`: pass.
- `python3 tests/conformance/runner/manifest_checker.py dry-run tests/conformance/manifests/conformance-v01.json`: pass.
- `git diff --check`: pass.
- ASCII scan over changed durable artifacts: pass.
- `.agents/sow/audit.sh`: first run found this SOW missing the explicit sensitive data gate; SOW text repaired and rerun passed.
- `SOW_AUDIT_SENSITIVE_CHANGED=1 bash .agents/sow/audit.sh`: pass.
- External review round 2: Minimax, Mimo, Qwen, and GLM returned `PRODUCTION GRADE` after the reopen monotonic fix and expanded tests. Kimi ran validation successfully but did not return a final verdict before hanging; this was recorded as a reviewer execution failure rather than a code finding.

## Outcome

Completed.

Delivered:

- Pure-Go `journal` module under `go/`.
- Writer API for creating, appending to, syncing, closing, and reopening journal files created by this writer.
- Regular, non-compact, uncompressed systemd journal file writer.
- Keyed data and field hash tables using the journal file ID.
- Data and field de-duplication.
- Entry object creation, entry arrays, and per-DATA entry links needed for file-backed filtering.
- Advisory one-writer file locking.
- Writer examples and Netdata-plugin-oriented README.
- Tests for hash vectors, field validation, writer lifecycle, de-duplication, hash collisions, entry-array growth, repeated-field filtering, journalctl read-back, journalctl verification, locking, close/sync behavior, and no-CGO validation.

Deferred by design:

- Go reader facade and Go journalctl rewrite.
- Compression, Forward Secure Sealing, compact format writer support, and appending to arbitrary historical/systemd-created journal variants.
- Cross-language reader/writer matrix, Node.js, Python, benchmarks, profiling, and optimization.

## Lessons Extracted

- Go reader facade and Go journalctl rewrite remain required after the writer-first SOW.
- Full cross-language writer/reader matrix remains required after all language implementations exist.

## Followup

- SOW-0010: complete Go reader facade and Go journalctl rewrite after writer-first delivery.
- SOW-0011: add mandatory live stock-reader and cross-language one-writer/multiple-reader compatibility gate before claiming the Go writer is production-compatible for concurrent reader operation.
- SOW-0008 or a dedicated writer feature SOW: add compression, Forward Secure Sealing, compact format support, and arbitrary historical-file append coverage.
- Add writer fault-injection tests for payload-size limits, short writes, disk-full behavior, and corruption recovery expectations.
- Document goroutine-safety expectations for all language writer APIs consistently.
- Extend field/value boundary tests, including exact 64-character positive cases and binary value payloads.

## Regression Log

None yet.
