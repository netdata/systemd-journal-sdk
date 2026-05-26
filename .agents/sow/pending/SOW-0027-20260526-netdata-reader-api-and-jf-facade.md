# SOW-0027 - Netdata Reader API And jf Facade

## Status

Status: open

Sub-state: Created from user request on 2026-05-26. Pending activation as a prerequisite for Netdata reader integration.

## Requirements

### Purpose

Define and implement the reader-side SDK contract needed by Netdata before replacing Netdata reader consumers. The contract must include a libsystemd-like compatibility facade based on Netdata's battle-tested `jf` crate, plus an idiomatic unified reader API that covers NetFlow, OTEL signal viewer, and systemd journal plugin needs across Rust, Go, Node.js, and Python.

### User Request

The user clarified that Netdata maintains a `jf` crate that provides an API similar to libsystemd, with near 1:1 mapping of libsystemd reader calls. This should be ported into the SDK for all languages if not already done. Then the project needs a reader-consumer analysis similar to the writer analysis: one superset/unified API for all Netdata readers, covering their different needs.

### Assistant Understanding

Facts:

- Netdata has a `src/crates/jf` implementation outside the main workspace. Evidence: `ktsaou/netdata @ 00305266364e`, `src/crates/Cargo.toml:3` excludes `jf`.
- Netdata `jf` exposes a C FFI-style libsystemd-compatible reader surface. Evidence: `ktsaou/netdata @ 00305266364e`, `src/crates/jf/journal_reader_ffi/src/lib.rs:105-222` implements open, close, seek, next, previous, and seek-realtime functions.
- Netdata `jf` has reader primitives for matches, conjunction/disjunction, field enumeration, unique field data, entry data enumeration, realtime, and seqnum. Evidence: `ktsaou/netdata @ 00305266364e`, `src/crates/jf/journal_file/src/reader.rs:58-193`.
- The SDK already has partial `SdJournal` facades in Rust, Go, Node.js, and Python. Evidence: `rust/src/journal/src/facade.rs:63-180`, `go/journal/facade.go:40-220`, `node/src/facade.js:133-285`, `python/journal/facade.py:1-220`.
- Existing SDK facades are not yet proven to be 1:1 with Netdata `jf` or libsystemd's reader-call expectations.

Inferences:

- Netdata reader integration should not depend only on the current SDK facade names. It needs a compatibility audit against `jf` and actual Netdata reader consumers.
- A single reader API may require multiple layers, as with writers: low-level file primitives, a libsystemd-compatible facade, and an idiomatic higher-level directory/query API.
- The `jf` crate is a strong reference for semantics, error codes, cursor/match behavior, and live-file robustness.

Unknowns:

- Which `jf` functions are actually called by current Netdata C/Rust code versus exposed for compatibility.
- Whether Go, Node.js, and Python should expose C-like function names exactly, idiomatic wrappers only, or both.
- Whether the existing SDK Rust reader should replace or wrap imported `jf` behavior, or whether specific `jf` code should be copied into this SDK as the reference implementation.

### Acceptance Criteria

- A reader-consumer inventory identifies every Netdata journal reader integration point in scope, including NetFlow reader/query/rebuild/facet paths, OTEL signal viewer reader paths, and systemd-journal plugin no-libsystemd reader paths.
- A `jf` API inventory maps Netdata `jf` functions and semantics to SDK Rust, Go, Node.js, and Python APIs.
- The SDK exposes a libsystemd-compatible reader facade in all four languages that covers the accepted `jf`/libsystemd reader subset, including open files/directories, close, seek head/tail/realtime/cursor where supported, next/previous/skip, add match, add conjunction/disjunction, flush matches, enumerate data, enumerate fields, query unique, get realtime, get monotonic/boot where applicable, get seqnum, get cursor/test cursor, and controlled unsupported behavior for daemon-only operations.
- The SDK exposes an idiomatic unified reader API in all four languages for Netdata use cases, separate from the compatibility facade where appropriate.
- The API explicitly supports binary field values, repeated fields, field-name remapping metadata, mixed directories, live one-writer/multiple-reader behavior, and compact/compressed/sealed files according to the product scope.
- The accepted reader contract is documented before Netdata integration work starts.
- Shared conformance tests compare SDK facade behavior with `jf` and stock libsystemd/journalctl behavior on synthetic fixtures where possible.
- Any operation not implemented is listed with evidence, reason, and a controlled error contract. No silent unsupported behavior is accepted.

## Analysis

Sources checked:

- `ktsaou/netdata @ 00305266364e`
  - `src/crates/Cargo.toml`
  - `src/crates/jf/journal_reader_ffi/src/lib.rs`
  - `src/crates/jf/journal_file/src/reader.rs`
  - `src/crates/netflow-plugin/src/query/scan/direct.rs`
  - `src/crates/netflow-plugin/src/query.rs`
  - `src/crates/netflow-plugin/src/facet_runtime.rs`
  - `src/crates/netdata-log-viewer/otel-signal-viewer-plugin/src/catalog.rs`
- SDK repository:
  - `rust/src/journal/src/facade.rs`
  - `go/journal/facade.go`
  - `node/src/facade.js`
  - `python/journal/facade.py`
  - `.agents/sow/pending/SOW-0026-20260526-netdata-sdk-integration.md`

Current state:

- Rust, Go, Node.js, and Python have partial `SdJournal` facades, but their coverage differs.
- Netdata `jf` is not simply a naming reference; it contains the reader semantics currently trusted for static Netdata builds.
- Netdata reader consumers use lower-level journal reader/index/query crates in several places, so a facade-only port may not be enough.

Risks:

- Directly integrating Netdata readers before this analysis can leave missing API gaps in the middle of the Netdata migration.
- A facade that has similar names but different error, cursor, match, or enumeration semantics can break Netdata behavior.
- Overfitting to one plugin can create a reader API that does not cover the systemd-journal plugin no-libsystemd mode.
- Porting only Rust `jf` semantics without cross-language tests can leave Go/Node.js/Python APIs inconsistent.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The writer API has received a Netdata consumer analysis, but reader API requirements have not. Netdata has a separate `jf` compatibility layer with libsystemd-like semantics, and the SDK's current facade layer is partial. A Netdata integration SOW needs this reader contract stabilized first.

Evidence reviewed:

- `ktsaou/netdata @ 00305266364e`, `src/crates/jf/journal_reader_ffi/src/lib.rs:105-222` shows C FFI-style reader calls.
- `ktsaou/netdata @ 00305266364e`, `src/crates/jf/journal_file/src/reader.rs:58-193` shows lower-level reader semantics for matching, seeking, field enumeration, unique queries, and data enumeration.
- `go/journal/facade.go:40-220` and `rust/src/journal/src/facade.rs:63-180` show the SDK already has facade work but not a recorded `jf` parity gate.

Affected contracts and surfaces:

- Rust, Go, Node.js, and Python reader facades.
- Rust, Go, Node.js, and Python idiomatic reader APIs.
- File-backed journalctl rewrites where they share facade/query behavior.
- Netdata NetFlow query/rebuild/facet reader paths.
- Netdata OTEL signal viewer reader paths.
- Netdata systemd-journal plugin no-libsystemd reader path.
- SOW-0026 Netdata SDK integration.

Existing patterns to reuse:

- Existing SDK `SdJournal` facades.
- Netdata `jf` reader and FFI semantics.
- Existing conformance adapters and journalctl rewrite tests.
- SOW-0023 writer-consumer analysis structure.

Risk and blast radius:

- High for Netdata integration because reader behavior affects query correctness, historical data access, and no-libsystemd deployments.
- Medium inside the SDK if the work is split into facade additions and idiomatic wrappers with shared tests.

Sensitive data handling plan:

- Use synthetic fixtures only. Do not copy real Netdata journal caches, customer logs, private endpoints, bearer tokens, SNMP community strings, personal data, or proprietary incident details into durable artifacts.

Implementation plan:

1. Inventory Netdata `jf` reader API and classify each operation as required, optional, daemon-only, unsupported, or replaced by a higher-level SDK API.
2. Inventory actual Netdata reader consumers and their semantic needs, similar to the SOW-0023 writer analysis.
3. Compare existing SDK facades in Rust, Go, Node.js, and Python against the accepted `jf`/libsystemd subset.
4. Design one cross-language reader API contract with two layers: compatibility facade and idiomatic SDK API.
5. Implement missing facade and idiomatic API pieces in all four languages.
6. Add shared conformance tests against synthetic fixtures, stock journalctl/libsystemd where applicable, and Netdata `jf` behavior where practical.
7. Update SOW-0026 to consume this reader contract before Netdata integration starts.

Validation plan:

- API inventory table with evidence-backed status for every accepted `jf`/libsystemd reader operation.
- Cross-language conformance tests for seeking, next/previous/skip, matches, OR/AND groups, data enumeration, field enumeration, query unique, cursor, realtime/monotonic/seqnum, repeated fields, binary values, remapped fields, and mixed directories.
- Stock journalctl/libsystemd comparisons for supported file-backed behavior.
- Netdata `jf` comparison tests or equivalent Rust fixture tests where direct linkage would not violate project constraints.
- Existing SDK reader, journalctl, compression, compact, FSS, and directory tests remain passing.
- External reviewer pass for API completeness, compatibility, and unwanted side effects.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: update journal compatibility skill if `jf` facade parity becomes mandatory for reader work.
- Specs: update product scope with the accepted reader API layers and `jf` compatibility status.
- End-user/operator docs: update SDK README/API docs in all languages.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: pending until activated; SOW-0026 depends on this SOW for reader integration.
- SOW-status.md: updated when created, activated, and closed.

Open-source reference evidence:

- `ktsaou/netdata @ 00305266364e`
  - `src/crates/Cargo.toml`
  - `src/crates/jf/journal_reader_ffi/src/lib.rs`
  - `src/crates/jf/journal_file/src/reader.rs`
  - `src/crates/netflow-plugin/src/query/scan/direct.rs`
  - `src/crates/netflow-plugin/src/query.rs`
  - `src/crates/netflow-plugin/src/facet_runtime.rs`
  - `src/crates/netdata-log-viewer/otel-signal-viewer-plugin/src/catalog.rs`

Open decisions:

- None blocking SOW activation. If the inventory shows a true API design fork, record numbered options and return to the user before implementation.

## Implications And Decisions

1. Reader integration prerequisite
   - Decision: SOW-0026 Netdata integration must depend on this reader API and `jf` facade parity SOW.
   - Reason: reader consumers are more diverse than writer consumers, and Netdata already has a proven compatibility layer that should shape the SDK contract.
   - Risk: skipping this step can force API churn during Netdata integration.

## Plan

1. Inventory Netdata `jf` and reader consumers.
2. Produce the unified reader API contract.
3. Implement missing facade and idiomatic API parity in all four languages.
4. Add shared conformance tests.
5. Update SOW-0026 dependency evidence and SDK docs/specs.

## Delegation Plan

Implementer:

- Current routing is local implementation by the project manager unless the user explicitly re-enables external implementers.

Reviewers:

- Use read-only reviewers from the approved pool: `llm-netdata-cloud/minimax-m2.7-coder`, `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/qwen3.6-plus`, and `llm-netdata-cloud/glm-5.1`. Skip `llm-netdata-cloud/mimo-v2.5-pro` while unavailable.

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

- Record implementation failures, reviewer failures, audit failures, API inventory gaps, and Netdata evidence gaps in this SOW before changing scope.

## Execution Log

### 2026-05-26

- Created SOW from user request while SOW-0023 review was running.
- Performed read-only evidence checks against `ktsaou/netdata @ 00305266364e` and existing SDK facade files.

## Validation

Pending activation and implementation.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.
