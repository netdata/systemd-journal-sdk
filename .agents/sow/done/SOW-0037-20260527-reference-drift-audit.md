# SOW-0037 - Writer Reference Closure

## Status

Status: completed

Sub-state: completed on 2026-05-28 after final closure review, audit, and
follow-up mapping.

## Requirements

### Purpose

Close the writer side cleanly before moving to reader optimization. The writer
contract must be conscious, evidence-based, and aligned in this order:

1. Rust writer remains as close to systemd journal behavior as practical.
2. Go writer follows Rust unless a difference is deliberately accepted.
3. Python and Node.js writer gaps are tracked as explicit follow-up SOWs.
4. Public writer API rules are the same across Rust, Go, Python, and Node.js.

### User Request

The user asked to review all pending work and convert the high-level plan into
clear SOWs. The user confirmed these priorities:

- close any remaining Rust writer gap against systemd now;
- confirm Go writer is aligned with Rust in the same areas;
- do not keep investigating why v0.1.0 was slow;
- do not start Netdata integration until writer and reader performance are fit
  for replacement;
- split reader work from writer work.

### Assistant Understanding

Facts:

- The project compatibility baseline is `systemd/systemd` tag `v260.1`.
- SOW-0038 shipped the three writer field-name policy modes: RAW, JOURNALD,
  and JOURNAL-APP.
- SOW-0036 shipped `live_publish_every_entries`, with `1` as stock-compatible
  default and `0` / `N > 1` as latency-tolerant performance options.
- Recent DATA payload caches were removed from the optimized Rust/Go writer
  path after measurement showed they did not improve throughput.
- SOW-0035 defined retention-derived rotation policy.
- Python writer still needs mmap/alignment work before writer completion.
- Node.js writer cannot rely on the same mmap primitive in the common runtime
  path, but its API and internal behavior must still align with Rust.

Inferences:

- Writer completion is not only performance. It includes policy, API, file
  layout, retention, publication, directory behavior, and compatibility
  evidence.
- Rust and Go should be certified first because they are the production hot
  path implementations for current Netdata work.
- Python and Node.js writer gaps should be separate SOWs so this SOW can close
  the reference decision without bundling all language implementation work.

Unknowns:

- Whether any Rust writer drift remains after the latest policy, retention,
  publication, and cache changes.
- Whether Go still has subtle writer differences from Rust in retention,
  publication, validation, compact output, compression, FSS, or structured/raw
  append behavior.

### Acceptance Criteria

- Produce an evidence-backed Rust writer versus systemd v260.1 closure matrix.
- Produce an evidence-backed Go writer versus Rust closure matrix.
- Confirm writer policy modes are identical across Rust and Go:
  RAW, JOURNALD, and JOURNAL-APP.
- Confirm retention-on-open, retention-derived rotation, max-size and
  max-duration defaults, active-file protection, and directory writer lifecycle
  are aligned for Rust and Go.
- Confirm compact/non-compact, compression on/off, mixed compression
  algorithms, FSS on/off, binary fields, open/closed journals, and live publish
  behavior remain covered by existing conformance or create follow-up SOWs for
  any gap.
- Confirm recent DATA cache removal is reflected in Rust and Go, or record an
  intentional difference with evidence.
- Confirm low-level raw full-payload and structured field append contracts are
  documented consistently.
- Do not optimize reader hot paths in this SOW.
- Do not investigate v0.1.0 slowness as a blocking target in this SOW.
- Update specs if writer contracts differ from current docs.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/SOW-status.md`
- `.agents/sow/current/SOW-0009-20260523-benchmark-profile-optimize.md`
- `.agents/sow/pending/SOW-0026-20260526-netdata-sdk-integration.md`
- `.agents/sow/done/SOW-0039-20260528-raw-byte-field-name-reader-representation.md`
  (closed and superseded by SOW-0043)
- `.agents/sow/specs/product-scope.md`
- `rust/src/crates/journal-core/src/file/file.rs`
- `rust/src/crates/journal-core/src/file/mmap.rs`
- `go/journal/writer.go`
- `go/journal/mmap_unix.go`
- `node/src/lib/writer.js`
- `python/journal/writer.py`

Open-source reference evidence:

- `systemd/systemd @ c0a5a2516d28`
  - `src/libsystemd/sd-journal/journal-file.c`
  - `src/libsystemd/sd-journal/mmap-cache.c`
  - `src/libsystemd/sd-journal/sd-journal.c`

Current state:

- Rust and Go are the writer priority implementations.
- Python and Node.js still need alignment work, now tracked separately.
- Reader work is intentionally split into separate reader parity and
  performance SOWs.
- Netdata integration remains blocked behind writer and reader performance.

Risks:

- If this SOW is too broad, it will blur writer certification with reader
  optimization and Netdata integration.
- If this SOW closes without a matrix, future changes may reintroduce drift
  without a durable reference.
- If Rust is not certified first, other languages may copy accidental behavior.

## Pre-Implementation Gate

Status: ready for writer-side audit and targeted fixes

Problem / root-cause model:

- The project has completed many compatibility SOWs. The remaining risk is
  fragmented knowledge: writer behavior is spread across specs, tests, SOWs,
  and implementations.
- A focused writer closure pass is required before reader optimization and
  Netdata integration can be judged against a stable writer contract.

Evidence reviewed:

- Current and pending SOW inventory listed in this file's analysis section.
- Product scope spec writer policy and directory writer sections.
- Rust/Go writer implementation files listed in this file's analysis section.
- systemd v260.1 source references listed above.

Affected contracts and surfaces:

- Rust and Go writer APIs.
- Writer field-name policies.
- Directory writer retention and rotation.
- Compact, compression, FSS, and live publication behavior.
- Binary field behavior and stock journalctl/libsystemd read compatibility.
- Specs and public README/API docs where writer contracts are documented.

Existing patterns to reuse:

- Shared conformance fixtures.
- Deterministic ingestion dataset and ingesters from SOW-0014/SOW-0015.
- Existing writer policy docs and tests from SOW-0038.
- Existing live publication tests from SOW-0036.
- Existing retention tests from SOW-0035.

Risk and blast radius:

- Medium for Rust/Go: writer behavior affects journal file compatibility and
  Netdata ingestion.
- High if retention or live publication changes are made without conformance
  validation.
- Low for Python/Node.js in this SOW because their implementation is only
  classified and delegated to follow-up SOWs.

Sensitive data handling plan:

- Use only synthetic fixtures and generated benchmark data.
- Do not record real hostnames, SNMP communities, customer data, personal data,
  credentials, bearer tokens, private endpoints, or production logs.

Implementation plan:

1. Build the Rust/systemd writer closure matrix from specs, code, tests, and
   systemd source evidence.
2. Build the Go/Rust writer closure matrix for the same surfaces.
3. Run targeted conformance and writer benchmark checks needed to prove the
   matrix.
4. Fix only Rust/Go writer drift discovered by the matrix, after recording any
   product decision that changes behavior.
5. Update specs/docs and close with reviewer passes.

Validation plan:

- Run relevant Rust and Go writer tests.
- Run shared writer conformance/interoperability tests for touched surfaces.
- Run stock `journalctl --verify --file` against generated outputs where the
  file is intended to be systemd-friendly.
- Run read-only reviewers on the full SOW and changed files.
- Search for same-failure patterns before close.

Artifact impact plan:

- AGENTS.md: no change expected unless a project-wide workflow rule changes.
- Runtime project skills: update compatibility skill only if a durable new
  writer workflow rule is discovered.
- Specs: update product-scope writer contracts if the audit changes or clarifies
  current behavior.
- End-user/operator docs: update README/API docs if public writer API wording
  changes.
- End-user/operator skills: no current output/reference skill expected.
- SOW lifecycle: keep this SOW current until writer closure is complete, then
  complete and move to done with implementation work in the same commit.
- SOW-status.md: update on activation and close.

Open-source reference evidence:

- `systemd/systemd @ c0a5a2516d28`
  - `src/libsystemd/sd-journal/journal-file.c`
  - `src/libsystemd/sd-journal/mmap-cache.c`
  - `src/libsystemd/sd-journal/sd-journal.c`

Open decisions:

- None blocking this SOW. The user agreed on 2026-05-28 to use this SOW as the
  writer closure checkpoint.

## Implications And Decisions

1. 2026-05-28 writer closure rescope
   - Decision: SOW-0037 is narrowed from broad reference drift to writer
     reference closure.
   - Implication: reader parity and reader performance move to separate SOWs.
   - Risk: writer closure may still discover reader-related evidence, but it
     must be tracked rather than implemented here.

2. 2026-05-28 v0.1.0 slowness
   - Decision: do not spend this SOW investigating why SDK v0.1.0 was slow.
   - Implication: v0.3.0 SNMP traps improvement remains useful integration
     evidence, not a root-cause requirement.

## Plan

1. Activate this SOW after the SOW restructuring commit.
2. Complete Rust/systemd writer closure matrix.
3. Complete Go/Rust writer closure matrix.
4. Fix or track every accidental writer drift.
5. Run writer validation and reviewer passes.
6. Update specs/docs and close.

## Delegation Plan

Implementer:

- Local implementation by the project manager unless the user changes routing.

Reviewers:

- Use read-only reviewers from the approved pool:
  `llm-netdata-cloud/minimax-m2.7-coder`, `llm-netdata-cloud/kimi-k2.6`,
  `llm-netdata-cloud/qwen3.6-plus`, and `llm-netdata-cloud/glm-5.1`.
  Skip `llm-netdata-cloud/mimo-v2.5-pro` while unavailable.

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

- Record matrix gaps, reviewer failures, audit failures, and benchmark failures
  in this SOW before changing scope.

## Execution Log

### 2026-05-28

- Rescoped SOW from broad reference drift to writer reference closure after the
  user agreed to split writer, reader, and Netdata integration work.
- Activated for implementation after the user approved proceeding.
- Writer API audit found a Go/Rust public API parity gap:
  - Rust exposes raw full-payload and structured direct-file append paths in
    `rust/src/crates/journal-core/src/file/writer.rs`.
  - Rust exposes raw and structured high-level `Log` write paths in
    `rust/src/crates/journal-log-writer/src/log/mod.rs`.
  - Go exposed structured `Append([]Field, EntryOptions)` but did not expose a
    raw full-payload `KEY=value` append shape for `Writer` or `Log`.
- Implemented the Go raw append shape:
  - `go/journal/writer.go`: added `Writer.AppendRaw` and shared append internals
    with structured `Append`.
  - `go/journal/log.go`: added `Log.AppendRaw`, including high-level
    `_SOURCE_REALTIME_TIMESTAMP` injection after caller field policy filtering.
  - `go/journal/field_policy.go`: added byte-slice field-name validation and
    raw payload policy preparation.
  - `go/journal/writer_test.go` and `go/journal/log_test.go`: added raw append
    coverage for JOURNALD, JOURNAL-APP, and RAW field-name policies.
  - `go/API.md`, `go/README.md`, and `go/journal/doc.go`: documented the new
    raw append public surface.
  - `.agents/sow/specs/product-scope.md`: clarified that raw full-payload APIs
    require the first `=` separator and reject malformed raw payloads before
    journal-app field-name filtering.
- Reviewer round 1 completed with `minimax`, `kimi`, `qwen`, and `glm`.
  Reviewers agreed the Go raw append implementation is production-grade, and
  reported low-risk follow-ups plus one Rust parity edge.
- Addressed reviewer round 1 findings:
  - Rust high-level `Log` journal-app raw filtering now rejects malformed raw
    payloads with no `=` separator or an empty field name instead of silently
    dropping them.
  - Added Rust regression coverage for malformed journal-app raw payloads.
  - Added Go byte-identity coverage proving structured `Append` and raw
    `AppendRaw` produce identical files for the same logical entry.
  - Added Go `AppendRaw` duplicate-payload deduplication coverage.
  - Clarified Go API/README wording that caller field policies apply to
    caller-provided fields/payloads, while SDK-owned fields/metadata use
    journald-compatible rules.
  - Dispositioned the pre-existing xor-hash-before-dedup observation as aligned
    with systemd v260.1; systemd computes `xor_hash` before
    `remove_duplicate_entry_items()` in
    `src/libsystemd/sd-journal/journal-file.c`.
- Reviewer round 2 completed with `minimax`, `kimi`, `qwen`, and `glm`.
  `kimi`, `qwen`, and `glm` marked the batch production-grade with low-risk
  observations. `minimax` returned findings citing non-existent Go symbols
  (`RawItem`, `filterForJournalApp`, `xorHashBeforeDedup`) and was rejected as
  not applicable to this repository.
- Addressed real reviewer round 2 findings:
  - Go high-level `Log` now injects indexed `_BOOT_ID=<boot-id>` DATA fields
    for structured and raw append paths, matching Rust and systemd journald's
    indexed boot-id behavior.
  - Go `Log` tests now verify the `_BOOT_ID` DATA payload exists for structured
    and raw high-level appends.
  - Go JOURNAL-APP raw tests now cover both malformed no-`=` and empty-name
    `=bad` payloads at direct-file and high-level `Log` layers.
  - Rust RAW policy tests now cover malformed no-`=` raw payloads.
  - Go API/README wording now states that high-level `Log` appends `_BOOT_ID`
    and `_SOURCE_REALTIME_TIMESTAMP` under journald-compatible rules.
  - SOW artifact maintenance text updated to reflect the product-scope spec and
    Go public docs changes.
- Reviewer round 3 completed with `qwen` and `glm`. Both marked the chunk
  production-grade with low-risk test coverage observations. `kimi` produced no
  final review after hanging in a read-only review run; the specific stalled
  process IDs were terminated without touching repository files.
- Addressed low-risk reviewer round 3 test coverage observations:
  - Go direct-file and high-level raw append tests now cover empty raw payloads
    and single-`=` payloads where applicable.
  - Go JOURNAL-APP high-level structured and raw tests now assert SDK-owned
    `_BOOT_ID` is still written after caller protected fields are dropped.
- Rejected the proposed high-level `Log` byte-for-byte structured/raw identity
  test as an invalid assertion. Evidence: `go/journal/log.go` intentionally
  clears `Options.FileID` in `ensureWriter`, so each directory-writer journal
  file receives a unique file ID even when the logical payload and caller
  options are identical. Direct-file `Writer` byte identity remains tested
  where the file ID is caller-controlled and deterministic.
- Built the writer closure matrix from systemd v260.1 source, Rust/Go code,
  tests, and focused interoperability runs.
- Focused validation results:
  - `journal-core` Rust tests passed: 54 tests plus doc-tests.
  - Go/Rust binary interoperability matrix passed 18/18.
  - Go/Rust zstd/xz/lz4 compression interoperability matrix passed 72/72.
  - Go/Rust compact/no-compression interoperability matrix passed 20/20.
  - Go/Rust regular live interoperability matrix passed 2/2, including stock
    libsystemd live readers.
  - Initial all-language lock matrix with `--entries 20 --delay-ms 1` failed
    because the holder process could close before later contenders ran. A
    manual Python-vs-Go lock probe and a longer matrix run with
    `--entries 200 --delay-ms 20` showed all SDK lock contention and stale-lock
    cleanup scenarios pass 8/8.

## Writer Closure Matrix

### Rust Versus systemd v260.1

Baseline source:

- `systemd/systemd @ c0a5a2516d28` (`v260.1` commit)

| Surface | systemd v260.1 evidence | Rust evidence | Status |
| --- | --- | --- | --- |
| JOURNALD field names | `src/libsystemd/sd-journal/journal-file.c:1710-1746` enforces non-empty, <=64 bytes, not digit-first, `A-Z0-9_`, and optional protected `_...` fields. | `rust/src/crates/journal-core/src/file/writer.rs:132-167` implements the same JOURNALD/JOURNAL-APP predicates. | Aligned. |
| RAW field names | DATA objects require a complete payload with a first `=` separator before a FIELD object is linked: `src/libsystemd/sd-journal/journal-file.c:1859-1875`, `1911-1914`. | `rust/src/crates/journal-core/src/file/writer.rs:147-153` requires non-empty field names and a raw payload separator. RAW permits names outside journald syntax but never `=` in the key. | SDK RAW is intentionally lower-level than systemd-friendly naming; format separator rule is aligned. |
| Structured/raw writer API | systemd `journal_file_append_entry()` accepts iovec payloads and writes DATA/FIELD/ENTRY objects: `src/libsystemd/sd-journal/journal-file.c:2527-2659`. | Rust exposes raw, structured, and mixed `EntryField` paths; byte identity is tested in `rust/src/crates/journal-core/src/file/writer.rs:1965-2039` and `2313-2327`. | Aligned and tested. |
| Duplicate DATA refs and xor hash | systemd computes xor hash before sorting/dedup, then sorts by object offset and removes duplicate entry items: `src/libsystemd/sd-journal/journal-file.c:2608-2631`. | Rust keeps default duplicate elimination and tests duplicate/trusted behavior in `rust/src/crates/journal-core/src/file/writer.rs:2249-2311`. | Aligned for default behavior; trusted fast path is explicit SDK extension. |
| Header/tail metadata and entry arrays | systemd links entries into the global entry array and updates head/tail realtime, monotonic, boot ID, and tail-entry offset: `src/libsystemd/sd-journal/journal-file.c:2253-2273`, `2386-2393`. | Rust updates the same header fields in `rust/src/crates/journal-core/src/file/writer.rs:801-816` and grows entry arrays in `1096-1184`. | Aligned and covered by stock verify plus binary/compact/live matrices. |
| Binary fields | systemd DATA payloads are raw bytes after `KEY=`; binary export/read behavior is validated by stock tooling. | Rust structured binary test is `rust/src/crates/journal-core/src/file/writer.rs:2041-2083`; Go/Rust binary matrix passed 18/18 against stock journalctl/libsystemd. | Aligned. |
| Compression | systemd attempts configured DATA compression only at/above threshold and keeps uncompressed data on failure/non-benefit: `src/libsystemd/sd-journal/journal-file.c:1808-1894`. | Rust compression path is `rust/src/crates/journal-core/src/file/writer.rs:896-949`; Go/Rust compression matrix passed 72/72 for zstd/xz/lz4. | Aligned for supported algorithms. |
| Compact format | systemd uses `HEADER_INCOMPATIBLE_COMPACT` and compact ENTRY/DATA layouts: `src/libsystemd/sd-journal/journal-def.h:168-184`. | Rust compact writer tests include stock verification and arena growth in `rust/src/crates/journal-core/src/file/writer.rs:2780-2886`; Go/Rust compact matrix passed 20/20. | Aligned. |
| FSS | systemd sets sealed flags and appends HMAC/TAG objects when sealing is enabled: `src/libsystemd/sd-journal/journal-file.c:413-425`, `2382-2399`, `2584-2588`. | Rust FSS writer tests cover stock verification, wrong-key failure, tamper failure, epoch gaps, empty sealed files, and compact+sealed in `rust/src/crates/journal-core/src/file/writer.rs:2347-2831`. | Aligned for SDK deterministic sealing API. |
| Live publication | systemd posts mmap changes by truncating the file to its current size after writes, or schedules a coalesced timer: `src/libsystemd/sd-journal/journal-file.c:2414-2506`, `2654-2657`. | Rust default publishes every entry and exposes `live_publish_every_entries`; implementation is `rust/src/crates/journal-core/src/file/writer.rs:748-761`. Regular live matrix passed 2/2 for Go/Rust with stock libsystemd readers. | Default aligned; non-1 cadence is an explicit latency-tolerant SDK extension. |
| Directory archive naming | systemd archives active files as `<source>@<seqnum-id>-<head-seqnum>-<head-realtime>.journal`: `src/libsystemd/sd-journal/journal-file.c:4359-4402`. | Rust default keeps Netdata-compatible chain naming and offers strict systemd active naming; covered in `rust/src/crates/journal-log-writer/tests/log_writer.rs:245-273`, `555-689`, `764-817`. | File naming compatibility is intentional: default follows Netdata; strict mode follows systemd active naming. |
| Retention and rotation | systemd computes default `max_use` from filesystem size and default `max_size = max_use / 8` capped at 128 MiB: `src/libsystemd/sd-journal/journal-file.c:4011-4063`; time rotation uses configured `max_file_usec`: `src/libsystemd/sd-journal/journal-file.c:4696-4708`; vacuum deletes oldest archived files by age/bytes/count while protecting active files: `src/libsystemd/sd-journal/journal-vacuum.c:179-195`, `295-321`. | Rust SDK uses the user-approved SDK contract: retention-derived rotation in 1/20 steps for bytes/age, explicit overrides, active-file protection, and retention-on-open. Tests are `rust/src/crates/journal-log-writer/tests/log_writer.rs:945-1159`, `1198-1581`. | Deliberate SDK-level policy difference; file format remains systemd-compatible. |
| Hash-table sizing and file identity | systemd reserves DATA buckets as `max(max_size * 4 / 768 / 3, 2047)` and FIELD buckets as `1023`: `src/libsystemd/sd-journal/journal-file.c:48-49`, `1279-1323`. | Rust creates hash tables and stores `file_id` in `rust/src/crates/journal-core/src/file/file.rs:1021-1076`; derived sizing tests are `rust/src/crates/journal-log-writer/tests/log_writer.rs:971-987`, `1064-1072`, `1088-1121`. | Aligned for the SDK effective max-file-size contract. |
| Timestamp validity policy | systemd rejects invalid realtime/monotonic input at append and can enforce strict ordering: `src/libsystemd/sd-journal/journal-file.c:2550-2558`, `2332-2359`. | Rust low-level writers preserve explicit caller timestamps for deterministic/corrupt-test files, while high-level `Log` clamps non-progressing timestamps; tests are `rust/src/crates/journal-log-writer/tests/log_writer.rs:1718-1803`, `1850-1900`. | Deliberate SDK API-layer split; high-level outputs remain stock-verifiable. |
| Open/closed journal state | systemd online/archive state and archive transition are in `src/libsystemd/sd-journal/journal-def.h:161-162` and `src/libsystemd/sd-journal/journal-file.c:4395-4402`. | Rust close/rotation/reopen tests cover online and archived paths in `rust/src/crates/journal-log-writer/tests/log_writer.rs:372-689`, `1271-1321`. | Covered. |
| DATA payload cache | systemd searches the on-file hash table for DATA objects; no SDK-style recent DATA payload cache is required for compatibility. | Rust no longer has a DATA payload cache. It still has a bounded FIELD-name cache in `rust/src/crates/journal-core/src/file/writer.rs:169-196`, which is not the high-cardinality DATA cache removed for performance. | Closed. |

### Go Versus Rust

| Surface | Rust reference | Go evidence | Status |
| --- | --- | --- | --- |
| Field-name policies | Rust `FieldNamePolicy` and validators are in `rust/src/crates/journal-core/src/file/writer.rs:86-167`; high-level journal-app raw filtering is in `rust/src/crates/journal-log-writer/src/log/mod.rs:35-71`, `764-782`. | Go policy implementation is `go/journal/field_policy.go:5-191`; tests are `go/journal/writer_test.go:320-654` and `go/journal/log_test.go:1680-1852`. | Aligned. |
| Raw and structured append | Rust raw/structured/mixed tests are `rust/src/crates/journal-core/src/file/writer.rs:1965-2039`. | Go direct writer exposes `Append` and `AppendRaw` in `go/journal/writer.go:300-332`; direct byte identity and duplicate tests are `go/journal/writer_test.go:392-492`. | Aligned for direct-file writer. |
| High-level `Log` metadata | Rust high-level `Log` injects `_BOOT_ID` and `_SOURCE_REALTIME_TIMESTAMP` before caller fields in `rust/src/crates/journal-log-writer/src/log/mod.rs:880-988`. | Go high-level `Log` injects the same SDK-owned fields in `go/journal/log.go:430-487`, `1110-1147`; tests assert stock-visible `_BOOT_ID` in `go/journal/log_test.go:1596-1769`. | Aligned. |
| Header/tail metadata and entry arrays | Rust updates header fields and grows global entry arrays in `rust/src/crates/journal-core/src/file/writer.rs:801-816`, `1096-1184`. | Go publishes object/entry metadata and grows entry arrays in `go/journal/writer.go:837-893`, `958-971`, `1343-1408`; basic header tests are `go/journal/writer_test.go:64-82`. | Aligned. |
| Hash-table sizing and file identity | Rust effective sizing is covered by the Rust/systemd matrix row. | Go uses the same formula in `go/journal/format.go:26-70`, applies it in `go/journal/writer.go:559-583`, and tests derived/explicit sizing in `go/journal/log_test.go:353-373`, `413-414`, `466-507`. | Aligned. |
| Timestamp validity policy | Rust low-level/high-level timestamp split is covered by the Rust/systemd matrix row. | Go low-level validation tests stock rejection for same-boot backward monotonic timestamps in `go/journal/writer_test.go:900-920`; high-level `Log` clamps source/entry timestamps in `go/journal/log.go:657-677` and `go/journal/log_test.go:1400-1497`, `1522-1560`. | Aligned for the SDK API-layer split. |
| Rotation and retention | Rust tests cover explicit and derived byte/time rotation, count/byte/age retention, active protection, and retention-on-open in `rust/src/crates/journal-log-writer/tests/log_writer.rs:855-1581`. | Go implements the same policy in `go/journal/log.go:240-276`, `657-702`, `771-855`, `1062-1108`; tests cover the same surfaces in `go/journal/log_test.go:285-1299`. | Aligned. |
| Compression and compact | Rust writer paths and tests are listed in the Rust/systemd table. | Go writer paths are `go/journal/writer.go:680-704`, `974-1004`; tests are `go/journal/writer_test.go:149-310`, `726-849`; Go/Rust matrices passed compression 72/72 and compact 20/20. | Aligned. |
| FSS | Rust FSS tests are `rust/src/crates/journal-core/src/file/writer.rs:2347-2831`. | Go FSS tests are `go/journal/seal_test.go:26-304` and verifier tests are `go/journal/verify_test.go:33-194`; full Go suite passed. | Aligned for current SDK sealing contract. |
| Live publication | Rust default and options are `rust/src/crates/journal-core/src/file/writer.rs:748-761`. | Go option and default are `go/journal/writer.go:61-65`, `593-604`; Go test is `go/journal/writer_test.go:96-148`; live matrix passed 2/2 for Go/Rust. | Aligned. |
| Writer lock | Rust/Go cooperative lock is SDK-defined because systemd does not mechanically prevent every possible external writer. | Lock matrix result `.local/interoperability/lock-matrix-results-20260528-201400.json` shows all Go, Rust, Node.js, and Python contenders fail while any SDK writer holds the lock, and stale lock cleanup passes. | All-language SDK lock contract aligned for the validated holder window. |
| DATA payload cache | Rust has no DATA payload cache; only FIELD cache remains. | Go has no DATA payload cache; it has a small direct-mapped FIELD cache in `go/journal/writer.go:520-555`, `1052-1097`. | Aligned in intent. |

### Tracked Gaps After This SOW

- Python writer must align API, mmap/file-access behavior, field policies, and
  raw/structured append behavior in SOW-0040.
- Node.js writer must align API, runtime-specific file-access behavior, field
  policies, and raw/structured append behavior in SOW-0041.
- Final all-language writer certification, including performance and profiling,
  remains SOW-0042.
- Reader parity and reader performance remain SOW-0043 through SOW-0046.

## Validation

Acceptance criteria evidence:

- Rust/systemd writer closure matrix is recorded in this SOW under
  `Rust Versus systemd v260.1`.
- Go/Rust writer closure matrix is recorded in this SOW under `Go Versus Rust`.
- Go now exposes both writer append shapes required by the writer API hierarchy:
  structured `Append([]Field, EntryOptions)` and raw full-payload
  `AppendRaw([][]byte, EntryOptions)` for both direct-file `Writer` and
  high-level directory `Log`.
- Rust high-level journal-app raw filtering now matches the SOW/spec rule that
  malformed raw payloads are rejected before journal-app field-name filtering.
- Go high-level `Log` now aligns with Rust/systemd journald behavior by writing
  `_BOOT_ID=<boot-id>` as an indexed DATA payload in addition to entry boot-id
  metadata.
- Python and Node.js writer parity gaps exposed during the closure pass are
  mapped to SOW-0040 and SOW-0041. The cooperative writer lock remains covered
  by the shared lock matrix rather than a known Python/Node.js bug.

Tests or equivalent validation:

- `GOCACHE=/home/costa/Documents/systemd-journal-sdk/.local/go-cache GOMODCACHE=/home/costa/Documents/systemd-journal-sdk/.local/go-mod-cache GOPATH=/home/costa/Documents/systemd-journal-sdk/.local/go-path go test ./...`
  from `go/` passed on 2026-05-28.
- `CARGO_HOME=/home/costa/Documents/systemd-journal-sdk/.local/cargo-home CARGO_TARGET_DIR=/home/costa/Documents/systemd-journal-sdk/.local/cargo-target cargo test --manifest-path rust/Cargo.toml -p journal-log-writer test_log_journal_app_policy_drops_invalid_fields -- --nocapture`
  passed on 2026-05-28.
- `CARGO_HOME=/home/costa/Documents/systemd-journal-sdk/.local/cargo-home CARGO_TARGET_DIR=/home/costa/Documents/systemd-journal-sdk/.local/cargo-target cargo test --manifest-path rust/Cargo.toml -p journal-log-writer`
  passed on 2026-05-28.
- After `_BOOT_ID` alignment and test additions, the same `go test ./...`,
  `cargo test --manifest-path rust/Cargo.toml -p journal-log-writer`, and
  `git diff --check` commands passed again on 2026-05-28.
- After round-3 coverage additions, the same `go test ./...`,
  `cargo test --manifest-path rust/Cargo.toml -p journal-log-writer`, and
  `git diff --check` commands passed again on 2026-05-28.
- `CARGO_HOME=/home/costa/Documents/systemd-journal-sdk/.local/cargo-home CARGO_TARGET_DIR=/home/costa/Documents/systemd-journal-sdk/.local/cargo-target cargo test --manifest-path rust/Cargo.toml -p journal-core`
  passed on 2026-05-28: 54 tests plus doc-tests.
- `GOCACHE=/home/costa/Documents/systemd-journal-sdk/.local/go-cache GOMODCACHE=/home/costa/Documents/systemd-journal-sdk/.local/go-mod-cache GOPATH=/home/costa/Documents/systemd-journal-sdk/.local/go-path python3 tests/interoperability/run_binary_matrix.py --writers go rust --readers stock go rust`
  passed on 2026-05-28: 18/18.
- `GOCACHE=/home/costa/Documents/systemd-journal-sdk/.local/go-cache GOMODCACHE=/home/costa/Documents/systemd-journal-sdk/.local/go-mod-cache GOPATH=/home/costa/Documents/systemd-journal-sdk/.local/go-path CARGO_HOME=/home/costa/Documents/systemd-journal-sdk/.local/cargo-home CARGO_TARGET_DIR=/home/costa/Documents/systemd-journal-sdk/.local/cargo-target python3 tests/interoperability/run_compression_matrix.py --writers go rust --readers stock go rust --compression zstd xz lz4 --entries 2`
  passed on 2026-05-28: 72/72.
- `GOCACHE=/home/costa/Documents/systemd-journal-sdk/.local/go-cache GOMODCACHE=/home/costa/Documents/systemd-journal-sdk/.local/go-mod-cache GOPATH=/home/costa/Documents/systemd-journal-sdk/.local/go-path CARGO_HOME=/home/costa/Documents/systemd-journal-sdk/.local/cargo-home CARGO_TARGET_DIR=/home/costa/Documents/systemd-journal-sdk/.local/cargo-target python3 tests/interoperability/run_compact_matrix.py --writers go rust --readers stock go rust --entries 2 --compression none`
  passed on 2026-05-28: 20/20.
- `GOCACHE=/home/costa/Documents/systemd-journal-sdk/.local/go-cache GOMODCACHE=/home/costa/Documents/systemd-journal-sdk/.local/go-mod-cache GOPATH=/home/costa/Documents/systemd-journal-sdk/.local/go-path CARGO_HOME=/home/costa/Documents/systemd-journal-sdk/.local/cargo-home CARGO_TARGET_DIR=/home/costa/Documents/systemd-journal-sdk/.local/cargo-target python3 tests/interoperability/run_live_matrix.py --writers go rust --readers stock go rust --features regular --entries 10 --poll-readers 1 --libsystemd-readers 1 --writer-delay-ms 5`
  passed on 2026-05-28: 2/2.
- `GOCACHE=/home/costa/Documents/systemd-journal-sdk/.local/go-cache GOMODCACHE=/home/costa/Documents/systemd-journal-sdk/.local/go-mod-cache GOPATH=/home/costa/Documents/systemd-journal-sdk/.local/go-path CARGO_HOME=/home/costa/Documents/systemd-journal-sdk/.local/cargo-home CARGO_TARGET_DIR=/home/costa/Documents/systemd-journal-sdk/.local/cargo-target python3 tests/interoperability/run_lock_matrix.py --entries 20 --delay-ms 1`
  failed the all-language contention summary on 2026-05-28, but that result is
  now classified as invalid for lock correctness because the holder can finish
  before later contenders run.
- Manual probe evidence on 2026-05-28: a Go holder kept
  `.local/sow0040-lock-probe/go-holder.journal.lock`, Python
  `_lock_file_is_stale()` returned `(False, 'pid ...')`, and Python
  `WriterLock.acquire()` raised `BlockingIOError` while the Go holder was
  still active.
- `GOCACHE=/home/costa/Documents/systemd-journal-sdk/.local/go-cache GOMODCACHE=/home/costa/Documents/systemd-journal-sdk/.local/go-mod-cache GOPATH=/home/costa/Documents/systemd-journal-sdk/.local/go-path CARGO_HOME=/home/costa/Documents/systemd-journal-sdk/.local/cargo-home CARGO_TARGET_DIR=/home/costa/Documents/systemd-journal-sdk/.local/cargo-target python3 tests/interoperability/run_lock_matrix.py --entries 200 --delay-ms 20`
  passed on 2026-05-28: 8/8. Essential result pattern from
  `.local/interoperability/lock-matrix-results-20260528-201400.json`: all four
  contention scenarios passed and all four stale-lock cleanup scenarios passed.

Real-use evidence:

- Go tests added in this SOW generate journal files and validate them with stock
  `journalctl --file` / `journalctl --directory` through existing helpers.
  Rust log-writer tests also exercise stock `journalctl --file` where the
  package's existing journalctl helpers require it.
- Focused interoperability runs generated Go/Rust binary, compressed, compact,
  and live journals and validated them with stock systemd 260.1 tooling,
  stock libsystemd helpers where applicable, and Go/Rust readers.

Reviewer findings:

- Round 1:
  - `minimax`: PRODUCTION GRADE for Go, found Rust journal-app raw malformed
    payload divergence, recommended Go byte-identity coverage, and raised
    pre-existing xor-hash-before-dedup observation.
  - `kimi`: PRODUCTION GRADE, no blocking issues; noted low-risk cleanup-only
    observations.
  - `qwen`: PRODUCTION GRADE, recommended explicit Go raw duplicate-payload
    dedup coverage and API wording clarification.
  - `glm`: PRODUCTION GRADE, no blocking issues; noted low-risk residuals.
- Dispositions:
  - Rust malformed raw payload divergence fixed and tested.
  - Go byte-identity and duplicate-payload dedup tests added.
  - Go API/README wording clarified.
  - xor-hash-before-dedup left unchanged because it matches systemd v260.1
    `systemd/systemd @ c0a5a2516d28`
    `src/libsystemd/sd-journal/journal-file.c:2608-2631`.
- Round 2:
  - `kimi`: PRODUCTION GRADE, flagged a real pre-existing `_BOOT_ID` indexed
    DATA payload parity gap and a low-risk Go JOURNAL-APP `=bad` test gap.
  - `qwen`: PRODUCTION GRADE, verified round-1 fixes; noted only low residuals.
  - `glm`: PRODUCTION GRADE, flagged Rust RAW no-`=` test coverage and SOW
    artifact maintenance inconsistency.
  - `minimax`: rejected as not applicable because its blocking findings cited
    symbols that do not exist in this repository: `RawItem`,
    `filterForJournalApp`, and `xorHashBeforeDedup`.
- Round 2 dispositions:
  - `_BOOT_ID` indexed DATA payload gap fixed in Go `Log` and tested.
  - Go JOURNAL-APP `=bad` tests added for `Writer.AppendRaw` and
    `Log.AppendRaw`.
  - Rust RAW no-`=` test added.
  - SOW artifact maintenance text corrected.
- Round 3:
  - `qwen`: PRODUCTION GRADE, with low-risk observations about log-level
    byte-identity coverage, JOURNAL-APP `_BOOT_ID` assertions, and duplicate
    `_BOOT_ID` behavior in trusted modes.
  - `glm`: PRODUCTION GRADE, with low-risk observations about empty/single-`=`
    raw payload tests and confirmation that double validation is acceptable.
  - `kimi`: no final review output; the read-only process hung and was
    terminated by specific PID after producing no actionable final review.
- Round 3 dispositions:
  - Empty and single-`=` raw payload coverage added where applicable.
  - JOURNAL-APP high-level `_BOOT_ID` assertions added.
  - High-level `Log` byte-for-byte identity rejected because directory writer
    files intentionally receive unique file IDs; direct-file `Writer` byte
    identity remains covered.
  - Duplicate caller-provided `_BOOT_ID` in JOURNALD/RAW trusted modes is
    accepted as consistent with the trusted writer contract and the existing
    Rust behavior; untrusted JOURNAL-APP caller `_BOOT_ID` is dropped before
    SDK-owned `_BOOT_ID` injection.
- Closure-matrix review round:
  - `qwen`: PRODUCTION GRADE with one medium evidence-quality finding. The
    systemd v260.1 citation used the annotated tag object instead of the peeled
    source commit.
  - `glm`: NOT PRODUCTION GRADE because the Python/Node.js writer lock gap was
    materially mischaracterized as missing/ignored locking instead of an
    existing-lock cross-process contention bug.
  - `kimi`: NOT PRODUCTION GRADE because terminal SOW sections were still
    pending, same-failure patterns were not synthesized, and low-risk
    follow-up/evidence-path cleanup remained.
- Closure-matrix review dispositions:
  - systemd evidence corrected to the peeled v260.1 commit
    `systemd/systemd @ c0a5a2516d28`.
  - Python/Node.js lock text was initially corrected across SOW-0037,
    SOW-0040, SOW-0041, and SOW-status.md from "missing lock" to "existing-lock
    contention bug". Later lock-matrix correction below supersedes this: the
    evidence does not support a Python/Node.js lock bug.
  - Essential lock matrix result patterns copied into this SOW so the
    `.local/` JSON files do not need to be preserved to understand the
    validation.
  - Matrix rows added for header/tail metadata, entry arrays, hash-table
    sizing/file identity, and timestamp policy.
  - Outcome, lessons, same-failure patterns, and terminal follow-up mapping
    completed before close.
- Final closure review round:
  - `qwen`: PRODUCTION GRADE. Low-severity observations only; no action
    required because essential `.local/` lock-matrix evidence is summarized in
    durable SOW text and SDK writer locks are not a systemd comparison surface.
  - `glm`: PRODUCTION GRADE. Verified the corrected lock-matrix wording: the
    runner summary is 8 scenarios, 4 passed, 4 failed, and contention details
    cover 16 contender attempts. This was later superseded by the
    post-close lock-matrix timing correction.
  - `kimi`: stalled in a read-only review run after repository reads and
    produced no final verdict; the specific reviewer process was terminated by
    PID and is not counted as a clean review.
- Final closure review disposition:
  - No blocking finding remained after `qwen` and `glm` reviewed the full
    closure scope and marked it production-grade. Because this final round was
    limited to validating SOW/status artifact corrections after prior
    multi-reviewer implementation rounds, two independent full-scope
    production-grade reviews were accepted as sufficient for close.

Same-failure scan:

- `rg` searches checked `_BOOT_ID` handling across Go/Rust tests, Go reader
  synthesis, Rust log writer injection, and systemd journald evidence. The
  remaining low-level `Writer` APIs intentionally do not inject `_BOOT_ID`;
  high-level `Log` now does.
- `rg` searches checked DATA cache references. Rust and Go retain bounded FIELD
  caches only; the removed high-cardinality DATA payload cache is not present.
- Lock matrix JSON was inspected after the all-language short-hold failure.
  Follow-up manual probe plus a longer lock matrix showed the short-hold
  failure was a test-duration artifact, not Node.js/Python contender behavior.
- Pattern: when a low-level append shape is added or changed, high-level `Log`
  metadata injection must be re-checked in every language. This SOW found the
  Go `_BOOT_ID` indexed-DATA gap only after raw/structured append parity work.
- Pattern: malformed raw payload behavior can diverge when one layer filters by
  dropping fields and another rejects before filtering. RAW and JOURNAL-APP
  tests must cover no-`=`, empty name, empty payload, and single-`=` payloads.
- Pattern: lock-matrix holder duration must be long enough for all sequential
  contenders to run while the holder is still alive. A too-short holder window
  can make later contenders look like lock violators after the holder has
  already closed cleanly.

Sensitive data gate:

- This SOW currently records only synthetic/planning evidence and source paths.
  No raw secrets, credentials, bearer tokens, SNMP communities, customer names,
  personal data, non-private customer-identifying IPs, private endpoints, or
  proprietary incident details were added.

Artifact maintenance gate:

- AGENTS.md: no update needed; no project-wide workflow rule changed.
- Runtime project skills: no update needed; compatibility rules already require
  raw and structured append shapes and policy parity.
- Specs: `.agents/sow/specs/product-scope.md` updated to clarify malformed raw
  full-payload rejection before journal-app filtering.
- End-user/operator docs: `go/API.md`, `go/README.md`, and `go/journal/doc.go`
  updated for the Go raw append API and high-level `Log` SDK-owned field
  behavior.
- End-user/operator skills: no output/reference skills affected.
- SOW lifecycle: this SOW is marked completed and moved to `done/` in the same
  commit as the final SOW/status artifact updates.
- SOW-status.md: updated when the SOW was activated and later corrected after
  the lock-matrix timing artifact was found.

Specs update:

- `.agents/sow/specs/product-scope.md` updated for raw full-payload malformed
  rejection semantics.

Project skills update:

- No project skill update needed for the rescope itself.

End-user/operator docs update:

- Go API and README docs updated for `AppendRaw`, caller-policy scope, and
  high-level `Log` SDK-owned field injection behavior.

End-user/operator skills update:

- No output/reference skill update needed.

Lessons:

- The writer and reader performance tracks must remain separate so benchmark
  work does not obscure compatibility closure.

Follow-up mapping:

- Python writer mmap/alignment work is tracked by SOW-0040.
- Node.js writer parity work is tracked by SOW-0041.
- Final all-language writer certification is tracked by SOW-0042.
- Reader parity and performance work is tracked by SOW-0043 through SOW-0046.

## Outcome

Rust/systemd and Go/Rust writer reference closure is complete for the surfaces
covered by this SOW. Go/Rust writer drift discovered during the SOW was fixed
or explicitly dispositioned, the writer closure matrix now records intentional
SDK policy differences, and remaining Python/Node.js writer parity work is
tracked by real follow-up SOWs.

## Lessons Extracted

- Annotated upstream tags must be peeled to the source commit before durable
  SOW citations are written.
- High-level writer metadata injection is easy to miss when adding low-level
  raw/structured append APIs; future parity work must check both layers.
- Cooperative writer locking must be validated cross-process and
  cross-language with holder durations that keep the holder alive throughout
  all contenders. Otherwise the test can produce a false lock-failure signal.

## Followup

- SOW-0040 - Python Writer Mmap And Rust Parity.
- SOW-0041 - Node.js Writer Rust Parity.
- SOW-0042 - Writer Final Certification.
- SOW-0043 - Rust Reader Libsystemd/Jf Parity.
- SOW-0044 - Rust Reader Hot-Path Optimization.
- SOW-0045 - Go Reader Alignment Optimization.
- SOW-0046 - Python Node Reader Alignment.

## Regression Log

### 2026-05-28 - Lock Matrix Timing Artifact

What broke:

- The close commit incorrectly recorded the short lock matrix failure
  `.local/interoperability/lock-matrix-results-20260528-194028.json` as a
  Python/Node.js cooperative writer lock bug.

Evidence:

- Manual Python-vs-Go probe under `.local/sow0040-lock-probe/` showed Python
  correctly treated a live Go holder lock as non-stale and raised
  `BlockingIOError`.
- Rerun with
  `python3 tests/interoperability/run_lock_matrix.py --entries 200 --delay-ms 20`
  passed 8/8 and wrote
  `.local/interoperability/lock-matrix-results-20260528-201400.json`.

Why previous validation missed it:

- The failing run used `--entries 20 --delay-ms 1`, so each holder could finish
  before later contenders were attempted. The runner records those later
  successful opens as contention failures even though the lock was already
  released.

Repair:

- SOW-0037, SOW-status, SOW-0040, and SOW-0041 were corrected to remove the
  unsupported Python/Node.js lock-bug claim.
- SOW-0040 and SOW-0041 still require lock-matrix validation as part of writer
  parity, but they no longer claim a known lock implementation bug.

Validation:

- `tests/interoperability/run_lock_matrix.py --entries 200 --delay-ms 20`
  passed 8/8 on 2026-05-28.

Reviewer validation:

- `glm`: PRODUCTION GRADE. Verified the short-hold failure pattern,
  8/8 longer lock matrix, SOW-0040/SOW-0041 lock-regression framing, sensitive
  data handling, and follow-up mapping.
- `qwen`: no final verdict; the read-only process stalled after inspecting the
  same correction scope and was terminated by specific PID.
- `minimax`: PRODUCTION GRADE. Verified the corrected conclusion is internally
  consistent, no current-reality text claims a known Python/Node.js lock bug,
  and SOW-0040/SOW-0041 require lock regression validation without claiming a
  bug fix.
