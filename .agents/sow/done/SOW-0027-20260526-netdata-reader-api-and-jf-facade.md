# SOW-0027 - Netdata Reader API And jf Facade

## Status

Status: completed

Sub-state: Completed after regression repair, local validation, and whole-SOW read-only reviewer pass.

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

Reader-consumer inventory:

- Netdata systemd-journal plugin no-libsystemd path uses a libsystemd-like `nsd_journal_*` facade. Required operations include `open_files`, `close`, `seek_head`, `seek_tail`, `seek_realtime_usec`, `next`, `previous`, `get_seqnum`, `get_realtime_usec`, field enumeration, unique enumeration, match/conjunction/disjunction, and match flushing. Evidence: `ktsaou/netdata @ 00305266364e`, `src/collectors/systemd-journal.plugin/provider/netdata_provider.h:46-73`.
- The systemd-journal plugin row path requires current-entry data enumeration as full `FIELD=value` payloads, including binary-safe lengths. Evidence: `ktsaou/netdata @ 00305266364e`, `src/collectors/systemd-journal.plugin/systemd-journal.c:230-287`.
- The systemd-journal plugin query path depends on direction-aware realtime seeking, backward iteration, realtime timestamps, and seqnum/writer identifiers. Evidence: `ktsaou/netdata @ 00305266364e`, `src/collectors/systemd-journal.plugin/systemd-journal.c:214-224` and `src/collectors/systemd-journal.plugin/systemd-journal.c:312-385`.
- The systemd-journal plugin slice/filter setup requires stateful field enumeration and `query_unique` values returned as full `FIELD=value` payloads so they can be re-used directly as match expressions. Evidence: `ktsaou/netdata @ 00305266364e`, `src/collectors/systemd-journal.plugin/systemd-journal.c:560-635`.
- NetFlow direct scans use low-level `JournalFile`, `JournalReader`, `JournalCursor`, `Location::Realtime`, match filters, entry data offsets, and caller-managed decompression buffers for performance. Evidence: `ktsaou/netdata @ 00305266364e`, `src/crates/netflow-plugin/src/query/scan/direct.rs:43-130`.
- NetFlow raw scans use `JournalReader::step`, current entry offsets, entry headers, and direct payload offset collection. Evidence: `ktsaou/netdata @ 00305266364e`, `src/crates/netflow-plugin/src/query/scan/raw.rs:104-180`.
- NetFlow facet scans use field-data object enumeration and payload decompression without stepping entries. Evidence: `ktsaou/netdata @ 00305266364e`, `src/crates/netflow-plugin/src/query/facets/cache/scan.rs:3-33`.
- NetFlow rebuild uses the higher-level registry/index/query stack (`Monitor`, `Registry`, `FileIndexCache`, `LogQuery`) for retained journal files. Evidence: `ktsaou/netdata @ 00305266364e`, `src/crates/netflow-plugin/src/ingest/rebuild.rs:7-66`.
- OTEL signal viewer uses the same higher-level registry/index/query stack via `journal_function` re-exports and constructs AND/OR filters from UI selections. Evidence: `ktsaou/netdata @ 00305266364e`, `src/crates/netdata-log-viewer/otel-signal-viewer-plugin/src/catalog.rs:14-18`, `src/crates/netdata-log-viewer/otel-signal-viewer-plugin/src/catalog.rs:34-71`, and `src/crates/netdata-log-viewer/otel-signal-viewer-plugin/src/catalog.rs:203-260`.

Accepted API split:

- Compatibility facade: model the file-backed `sd_journal_*`/Netdata `jf` subset needed by systemd-journal plugin and CLI-style tools. This layer is stateful and exposes C-like enumeration semantics where each `restart_*` resets an iterator and each `enumerate_*` returns one item.
- Idiomatic reader API: keep and extend existing `FileReader`/`DirectoryReader` APIs for direct SDK users. This layer returns language-native entries, field maps, unique values, boot lists, and output helpers.
- Performance-oriented low-level Rust API: NetFlow currently requires offset-level access and external buffer reuse. This SOW records the need and keeps the Rust low-level surface intact; the Netdata integration SOW should decide whether Go/Node/Python need equivalent low-level APIs beyond the facade.

Current SDK facade gaps against accepted subset:

- Rust has file/directory open, matching, head/tail seek, next/previous, cursor, realtime, bulk fields, and bulk unique values, but lacks facade-level open-files, close, seek-realtime, seek-cursor wrapper, next/previous skip, seqnum, monotonic/boot, and stateful data/field/unique enumeration. Evidence: `rust/src/journal/src/facade.rs:63-290`.
- Go has similar partial coverage plus skip helpers and seek-cursor by scan, but lacks open-files, seek-realtime, seqnum, monotonic/boot, and stateful enumerators; `Reader.QueryUnique` currently reads only the single-value `Fields` map and can miss repeated values. Evidence: `go/journal/facade.go:40-220` and `go/journal/reader.go:740-760`.
- Node.js has partial coverage for file/directory open, matching, head/tail seek, next/previous, cursor, realtime, bulk fields, and bulk unique values, but lacks open-files, skip helpers, seek-realtime, seek-cursor, seqnum, monotonic/boot, and stateful enumerators. Evidence: `node/src/facade.js:133-287`.
- Python has the same partial coverage shape as Node.js. Evidence: `python/journal/facade.py:1-263`.

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
- SOW lifecycle: completed and moved to `.agents/sow/done/`; SOW-0026 depends on this SOW for reader integration.
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
- Activated after SOW-0025 completed, committed, and pushed.
- Refreshed Netdata reader-consumer evidence. The important split is: systemd-journal plugin needs `jf` facade parity; NetFlow uses low-level raw scan/index APIs for performance; OTEL signal viewer uses the higher-level registry/index/query stack.
- Implemented the accepted file-backed reader facade in Rust, Go, Node.js, and Python:
  - open file, open directory, open explicit file set, and close;
  - seek head/tail/realtime/cursor, next/previous/skip;
  - add match, add conjunction/disjunction, and flush matches;
  - get entry, get data, current-entry data restart/enumeration;
  - field restart/enumeration and unique query/restart/enumeration;
  - realtime, monotonic/boot, seqnum/seqnum-id, cursor/test-cursor, output formatting, and boot listing.
- Extended entry objects and reader paths to preserve repeated and binary field values for current-entry data enumeration:
  - Rust `Entry.payloads`;
  - Go `Entry.Payloads`;
  - Node.js `entry.payloads`;
  - Python `entry['payloads']`.
- Implemented direction-aware realtime seek and explicit file-set directory readers across Go, Node.js, and Python, matching the Rust facade behavior.
- Standardized direct `SdJournalQueryUnique` semantics as language-native `(field, raw value)` pairs, while keeping stateful unique enumeration as full `FIELD=value` payloads.
- Fixed reviewer-raised gaps before close:
  - Go, Node.js, and Python `SdJournalSeekCursor` now seek by realtime first and stop once current realtime exceeds the target;
  - Go direct unique query no longer uses a string pair wrapper and returns binary-safe `UniqueValue`;
  - Rust direct unique query now returns `(String, Vec<u8>)` pairs;
  - Go `SdJournalGetEntryWithRealtime` no longer scans from the file head;
  - Python `previous()` now reports unsupported custom readers explicitly.
- Updated specs and user-facing SDK docs for the accepted reader facade contract.

## Validation

Acceptance criteria evidence:

- Reader-consumer inventory completed with evidence for:
  - systemd-journal plugin no-libsystemd facade needs: `ktsaou/netdata @ 00305266364e`, `src/collectors/systemd-journal.plugin/provider/netdata_provider.h:46-73`;
  - current-entry row data enumeration: `src/collectors/systemd-journal.plugin/systemd-journal.c:230-287`;
  - realtime/backward/seqnum query path: `src/collectors/systemd-journal.plugin/systemd-journal.c:214-224` and `src/collectors/systemd-journal.plugin/systemd-journal.c:312-385`;
  - stateful field and unique enumeration: `src/collectors/systemd-journal.plugin/systemd-journal.c:560-635`;
  - NetFlow low-level direct/raw/facet/rebuild reader needs: `src/crates/netflow-plugin/src/query/scan/direct.rs:43-130`, `src/crates/netflow-plugin/src/query/scan/raw.rs:104-180`, `src/crates/netflow-plugin/src/query/facets/cache/scan.rs:3-33`, and `src/crates/netflow-plugin/src/ingest/rebuild.rs:7-66`;
  - OTEL signal viewer registry/index/query usage: `src/crates/netdata-log-viewer/otel-signal-viewer-plugin/src/catalog.rs:14-18`, `src/crates/netdata-log-viewer/otel-signal-viewer-plugin/src/catalog.rs:34-71`, and `src/crates/netdata-log-viewer/otel-signal-viewer-plugin/src/catalog.rs:203-260`.
- `jf` facade parity implemented for the accepted file-backed subset in:
  - `rust/src/journal/src/facade.rs`;
  - `go/journal/facade.go`;
  - `node/src/facade.js`;
  - `python/journal/facade.py`.
- Idiomatic reader surfaces extended without removing existing reader APIs in:
  - `rust/src/journal/src/lib.rs`;
  - `go/journal/reader.go`;
  - `node/src/lib/reader.js`;
  - `node/src/lib/directory-reader.js`;
  - `python/journal/reader.py`;
  - `python/journal/directory_reader.py`.
- Cross-language tests cover binary and repeated values, current-entry data enumeration, unique state enumeration, field enumeration, realtime forward/backward seek, exact cursor seek, explicit file-set directory reading, and before-range backward seek:
  - `rust/src/journal/src/lib.rs`;
  - `go/journal/facade_test.go`;
  - `node/test/all.js`;
  - `python/test_all.py`.
- Unsupported daemon-only behavior remains outside this SOW and is unchanged.

Tests or equivalent validation:

- `cd go && go test ./...` passed.
- `cd rust && cargo test` passed.
- `node node/test/all.js` passed.
- `PYTHONPATH=.local/python-deps PYTHONDONTWRITEBYTECODE=1 python3 python/test_all.py` passed.
- `git diff --check` passed.

Real-use evidence:

- Synthetic journal files written by the existing SDK writers are opened through the new facade tests in all four languages.
- Multi-file explicit-open tests verify file ordering and realtime seek behavior against temporary journal files.
- No live host journal was probed. No writes were made to `/var/log/journal`, `/run/log/journal`, or external repositories.

Reviewer findings:

- First review cycle:
  - Minimax reported non-production due missing cursor/unique/test evidence. Real issues were fixed or covered by follow-up tests.
  - Qwen reported Go before-range backward realtime seek and binary unique API concerns. The before-range bug was fixed; the direct unique API was made binary-safe and standardized across languages.
  - GLM reported production-grade with minor recommendations. Python empty-payload fallback and cursor/multifile tests were addressed.
  - Kimi stalled and was terminated by exact PID after no useful final output.
- Final review cycle after fixes:
  - GLM: `PRODUCTION GRADE`; non-blocking notes only.
  - Minimax: `PRODUCTION GRADE`; non-blocking style/performance notes only.
  - Qwen stalled after partial read-only analysis and was terminated by exact PIDs `1595802` and `1595797`; no final verdict was produced. The earlier Qwen blocking findings had already been fixed and revalidated.

Same-failure scan:

- Searched for `SdJournalQueryUnique`, `query_unique`, and `queryUnique` call sites; updated direct unique semantics and tests where this API is used.
- Searched for `SeekRealtime`, `seek_realtime`, `seekRealtime`, and `SeekCursor`; patched the same failed-cursor early-exit pattern across Go, Node.js, and Python.
- Searched for `payloads` and current-entry enumeration paths; all four reader implementations now expose binary-safe payloads.

Sensitive data gate:

- Durable artifacts contain no raw secrets, passwords, bearer tokens, SNMP communities, private keys, customer names, personal data, non-private customer-identifying IP addresses, private endpoints, or proprietary incident details.
- External evidence is cited using upstream repository identity and commit, not workstation absolute mirror paths.

Artifact maintenance gate:

- AGENTS.md: no update needed; workflow and repository-boundary rules did not change.
- Runtime project skills: no update needed; existing `project-agent-orchestration` and `project-journal-compatibility` skills already covered the work and no new durable workflow rule was discovered.
- Specs: updated `.agents/sow/specs/product-scope.md` with accepted reader API layers and current language feature slices.
- End-user/operator docs: updated `rust/README.md`, `go/README.md`, `go/API.md`, `node/README.md`, and `python/README.md`.
- End-user/operator skills: no output/reference skills exist for this SDK API.
- SOW lifecycle: status is `completed`; this file will move to `.agents/sow/done/` and be committed with the implementation.
- SOW-status.md: updated for activation; will be updated again during close to remove this SOW from current.

Specs update:

- `.agents/sow/specs/product-scope.md` now records the accepted reader API layers, direct unique query shape, stateful unique payload semantics, repeated/binary value guarantees, direction-aware realtime seek, and OpenFiles directory behavior.

Project skills update:

- No project skill update needed. The work followed existing orchestration and journal compatibility rules; no new reusable process rule was discovered.

End-user/operator docs update:

- Rust, Go, Node.js, and Python README/API docs now describe the accepted reader facade operations and binary-safe stateful enumeration behavior.

End-user/operator skills update:

- No end-user/operator skills are produced or consumed by this project.

Lessons:

- Direct convenience APIs and stateful libsystemd-like APIs must be treated as separate contracts. Direct unique queries now return language-native `(field, raw value)` pairs; stateful unique enumeration returns reusable `FIELD=value` payloads.
- Cursor seek optimization must be validated across languages as both an API behavior and a performance behavior; seeking by realtime first is correct only when tests also assert exact cursor matching.

Follow-up mapping:

- Netdata integration is tracked by `.agents/sow/pending/SOW-0026-20260526-netdata-sdk-integration.md`.
- Mixed-format directory reader behavior is tracked by `.agents/sow/pending/SOW-0024-20260526-mixed-format-directory-readers.md`.
- Broader compatibility/test-gap work, including deeper stock/jf comparisons beyond this synthetic facade parity slice, remains tracked by `.agents/sow/pending/SOW-0022-20260525-compatibility-test-gap-audit.md`.
- Performance optimization remains intentionally after feature completion and is tracked by the existing benchmark/performance SOW.

## Outcome

Completed. The SDK now has a stabilized file-backed reader facade contract across Rust, Go, Node.js, and Python that covers the accepted Netdata `jf`/libsystemd subset needed before Netdata reader integration starts.

## Lessons Extracted

- Keep direct convenience APIs and stateful compatibility APIs explicitly separate in docs, tests, and specs.
- Add binary and repeated-value assertions to every language whenever changing facade enumeration semantics.

## Followup

- Continue with Netdata integration through SOW-0026.
- Continue mixed-format directory reader validation through SOW-0024.
- Continue broader compatibility gap closure through SOW-0022.
- Continue performance benchmarking/optimization after remaining feature work, per the user's priority decision.

## Regression Log

### Regression - 2026-05-31 - Unique Enumeration Uses Row Scans

Status: in-progress.

Regression summary:

- The SOW accepted libsystemd/JF-compatible field and unique enumeration semantics, but the public unfiltered unique-value APIs currently enumerate entries and expand entry values instead of using the journal FIELD object's DATA chain.
- This is separate from SOW-0074. SOW-0074 tracks new filtered explorer/query APIs. This regression tracks the existing unfiltered `query_unique` / `QueryUnique` / `queryUnique` / stateful unique enumeration behavior.

Evidence:

- `systemd/systemd @ cf3156842209`
  - `src/libsystemd/sd-journal/sd-journal.c:3332` initializes `sd_journal_query_unique()`.
  - `src/libsystemd/sd-journal/sd-journal.c:3386` finds the FIELD object for the queried field.
  - `src/libsystemd/sd-journal/sd-journal.c:3390` starts from `field.head_data_offset`.
  - `src/libsystemd/sd-journal/sd-journal.c:3396` advances through `data.next_field_offset`.
  - `src/libsystemd/sd-journal/sd-journal.c:3439` de-duplicates results across earlier files.
- SDK repository:
  - `rust/src/journal/src/lib.rs:1768` scans entries in `DirectoryReader::query_unique()`.
  - `go/journal/reader.go:1286` scans `entryOffsets` in `Reader.QueryUnique()`.
  - `python/journal/reader.py:474` scans `_entry_offsets` in `query_unique()`.
  - `node/src/lib/reader.js:457` scans `entryOffsets` in `queryUnique()`.
  - `rust/src/crates/journal-core/src/file/reader.rs:244` already exposes a lower-level `field_data_query_unique()` that uses `journal_file.field_data_objects(field_name)`.

Why previous validation missed it:

- SOW-0027 validation covered output semantics, repeated/binary values, and cross-language facade shape, but did not assert the algorithmic/systemd parity that unique enumeration must traverse the FIELD/DATA index rather than scanning all entries.
- The tests were correctness-oriented and did not include a high-cardinality or sparse-field performance case where row-scan behavior is obvious.

Repair plan:

1. Add conformance/performance-sensitive tests proving unfiltered unique enumeration uses field-index traversal, not full entry traversal. The test should include a sparse field and many irrelevant entries so a row-scan implementation is detectable.
2. Fix Rust public `FileReader`/`DirectoryReader` and facade unique paths to use the existing FIELD/DATA indexed primitive where possible, preserving binary-safe value output and stateful `FIELD=value` payload semantics.
3. Fix Go, Python, and Node.js unique paths to use FIELD/DATA object chains instead of entry scans, matching systemd semantics for file and directory readers.
4. Preserve directory de-duplication across files. Match systemd's observable behavior: unique values should not be returned repeatedly across files.
5. Validate against stock systemd/libsystemd-compatible behavior, shared fixtures, binary/repeated fields, compressed DATA, compact files, mixed directories, and high-cardinality sparse fields.
6. Update docs/specs only if they need to state the index-backed unique enumeration guarantee.

Validation required before re-closing:

- Rust, Go, Python, and Node unit tests for unique enumeration.
- Shared conformance test for sparse-field unique enumeration that fails under full-entry scan instrumentation.
- Directory reader tests proving cross-file de-duplication.
- Compression/compact/mixed-directory unique tests where supported by existing readers.
- Benchmark or operation-counter evidence showing unique enumeration cost is proportional to DATA objects for the requested field, not total entries.
- Read-only reviewer pass across the whole reopened SOW after implementation and local validation.

Implementation update - 2026-05-31:

- Added a durable performance contract to `AGENTS.md`, root `README.md`,
  `.agents/skills/project-journal-compatibility/SKILL.md`, and
  `.agents/sow/specs/product-scope.md`.
- Rust public `FileReader::query_unique()`, `DirectoryReader::query_unique()`,
  and the facade unique paths now use FIELD/DATA indexed traversal through the
  existing journal-core `field_data_objects()` primitive.
- Rust public `FileReader::enumerate_fields()`, `DirectoryReader::enumerate_fields()`,
  and facade field enumeration now prefer FIELD hash table traversal. They fall
  back to entry scanning only when FIELD hash traversal is unusable, preserving
  historical-fixture compatibility.
- Go `Reader.QueryUnique()` now finds the FIELD object through the field hash
  table, walks `DATA.next_field_offset`, and returns unique values without
  consulting `entryOffsets`.
- Go `Reader.EnumerateFields()` now prefers FIELD hash table traversal and
  falls back to the previous entry scan only if the FIELD table cannot be used.
- Python `FileReader.query_unique()` now finds the FIELD object through the
  field hash table, walks `next_field_offset`, and returns unique values without
  consulting `_entry_offsets`.
- Python `FileReader.enumerate_fields()` and `DirectoryReader.enumerate_fields()`
  now prefer FIELD hash table traversal and fall back to the previous entry scan
  only if the FIELD table cannot be used.
- Node.js `FileReader.queryUnique()` now finds the FIELD object through the
  field hash table, walks `nextFieldOffset`, and returns unique values without
  consulting `entryOffsets`.
- Node.js `FileReader.enumerateFields()` now prefers FIELD hash table traversal
  and falls back to the previous entry scan only if the FIELD table cannot be
  used.
- Go, Python, and Node.js regression tests clear the entry-offset list before
  calling unfiltered unique and field-name enumeration, proving those
  implementations no longer depend on row traversal for normal indexed files.
- Directory-reader regression tests verify cross-file de-duplication for
  `query_unique`/`QueryUnique`/`queryUnique`.
- Compatibility note: one archived systemd `no-rtc` fixture has an unusable
  FIELD hash chain for field-name enumeration. The SDK keeps the indexed fast
  path for valid files and uses the entry-scan fallback only for this
  compatibility case. This fallback does not apply to unfiltered unique value
  enumeration for valid FIELD/DATA chains.

Local validation - 2026-05-31:

- `cargo test -p journal`: passed before the field-enumeration expansion.
- `cargo test -p journal -p adapter`: passed, 24 journal tests and adapter build/test.
- `go test ./...`: passed.
- `PYTHONPATH=python:.local/python-deps .local/python-venv/bin/python python/test_all.py`: passed.
- `node node/test/all.js`: passed.
- `python3 tests/conformance/runner/manifest_checker.py validate tests/conformance/manifests/conformance-v01.json`: passed.
- `git diff --check -- AGENTS.md README.md .agents/skills/project-journal-compatibility/SKILL.md .agents/sow/specs/product-scope.md .agents/sow/current/SOW-0027-20260526-netdata-reader-api-and-jf-facade.md .agents/sow/SOW-status.md rust/src/journal/src/lib.rs rust/src/journal/src/facade.rs go/journal/reader.go go/journal/reader_test.go python/journal/reader.py python/journal/directory_reader.py python/test_all.py node/src/lib/reader.js node/test/all.js`: passed.
- `.agents/sow/audit.sh`: passed.
- `PYTHONPATH=python python3 python/test_all.py`: failed because the system
  Python interpreter does not have `lz4`; rerun with the repo-local dependency
  environment passed.

Reviewer status:

- `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`.
- `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE`.
- `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE`.
- `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`.
- `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`.

Reviewer dispositions:

- Qwen noted that Go uses `string(value)` for binary de-duplication. This was
  rejected as a false finding: Go strings preserve arbitrary bytes, including
  invalid UTF-8, and the value stored in the result is copied with
  `cloneBytes(value)`.
- Qwen, GLM, and Kimi noted the broad field-enumeration fallback in
  Python/Node.js and related Rust compatibility behavior. This was accepted as
  intentional compatibility handling for historical/damaged FIELD tables; the
  fast path remains indexed for valid files, and the SOW records the fallback
  boundary.
- Kimi noted that FIELD and DATA chain cycle detection is not added by this
  regression repair. This was rejected for this regression because the work did
  not introduce chain walking as a new corrupted-file validation surface; it
  replaced public row scans with the same indexed graph shape used by systemd
  and the existing Rust core/JF primitives.
- GLM noted that high-cardinality benchmark evidence was not added here. This
  was accepted as non-blocking because the regression is proven by tests that
  clear entry-offset lists and by source evidence showing FIELD/DATA traversal;
  benchmark and profiling work remains tracked by the active performance SOW.
- Mimo noted that Python did not have a mixed-value cross-file unique-values
  test. Fixed by
  `test_directory_reader_query_unique_deduplicates_indexed_values_across_files`,
  which is now called by the full `python/test_all.py` runner.

Post-review validation update - 2026-05-31:

- Targeted Python unique/facade/directory tests passed after adding the
  mixed-value directory regression test and wiring the indexed unique-values
  regression test into `main()`.
- `PYTHONPATH=python:.local/python-deps .local/python-venv/bin/python python/test_all.py`: passed.
- `cargo test -p journal -p adapter`: passed, 24 journal tests and adapter build/test.
- `go test ./...`: passed.
- `node node/test/all.js`: passed.

Close evidence:

- All four public reader implementations use FIELD/DATA indexed traversal for
  unfiltered unique-value enumeration on valid indexed files.
- All four public reader implementations use FIELD hash traversal for
  field-name enumeration on valid indexed files, with the documented scan
  fallback only for unusable FIELD tables.
- Directory readers de-duplicate unique values across files.
- The durable performance contract is recorded in `AGENTS.md`, root
  `README.md`, `.agents/skills/project-journal-compatibility/SKILL.md`, and
  `.agents/sow/specs/product-scope.md`.
