# SOW-0037 - Reference Drift Audit

## Status

Status: paused

Sub-state: paused on 2026-05-28 because SOW-0038 is now the active critical
path for the SNMP traps `v0.3.0` release. SOW-0036 completed the selected
cross-language `live_publish_every_entries` API. SOW-0009 broad benchmarking
remains paused until SOW-0037 resumes the wider reference-drift audit and
closes.

## Requirements

### Purpose

Make implementation differences conscious and evidence-based. Rust should stay
as close to systemd journal behavior as practical, with every deviation recorded
as intentional, measured, and compatible. Go, Node.js, and Python should then use
Rust as the project reference unless a language-specific difference is explicitly
accepted. Public SDK APIs across languages must also converge on the same
conceptual model. The audit must explicitly decide how the systemd-compatible
full-payload `KEY=value` API layer and any structured key/value convenience
layer relate to each other.

### User Request

The user asked to check for important differences first between systemd and
Rust, and then between Go and Rust. The user stated that Rust should remain as
close to systemd as possible, or differences must be conscious rather than
accidental. Rust should then become the reference for Go, and later for Node.js
and Python.

The user then added that APIs between languages must be similar. The standard
writer API should be structured for performance. Helpers may convert text
`KEY=value` data to structured fields, but text/raw data must not be the
mandatory or canonical API shape.

Follow-up evidence from systemd v260.1 showed that systemd's public send API
and internal journal-file append API use full `KEY=value` field payloads in
`struct iovec` arrays, not separate key/value structs. This reopens the exact
API hierarchy decision: the SDK can still expose structured helpers, but the
systemd-compatible low-level layer is raw full field payloads.

The user then pointed out that NetFlow, SNMP traps, and OTEL already have
structured fields, so converting to full `KEY=value` bytes and then parsing
those bytes again is wasteful. The user also noted that a structured path could
avoid sorting and deduplication when the caller knows the entry already satisfies
the relevant invariants.

On 2026-05-28 the user challenged the Rust and Go recent DATA payload caches.
Evidence showed systemd v260.1 does not have an equivalent recent DATA payload
cache, and Rust benchmark experiments showed that the cache did not improve
throughput despite high hit ratios. The user decided to remove the recent DATA
cache from Rust and requested an equivalent Go cache/no-cache test before
deciding whether Go should follow.

### Assistant Understanding

Facts:

- The project compatibility baseline is `systemd/systemd` tag `v260.1`.
- Rust already acts as the strongest SDK implementation and contains the
  imported Netdata-compatible journal implementation plus the newer SDK layers.
- Known examples of difference already surfaced:
  - systemd and Rust use windowed mmap object access; Go writer currently maps
    the whole allocated file on Unix.
  - Go reader currently uses `ReadAt()` buffers rather than mmap-backed access.
  - systemd has mmap-cache categories, unused-window retention, SIGBUS
    invalidation, and optional post-change timer coalescing; Rust has a simpler
    `WindowManager` model.
  - systemd uses stock journal naming and daemon-oriented conventions; the SDK
    has Netdata-compatible naming as default, with strict systemd naming as an
    explicit compatibility mode where required.
- Existing byte-identity, structural, live, compression, compact, FSS, verify,
  directory, and journalctl matrices already cover many file-format behaviors.
- systemd's documented writer-facing send API uses full `VARIABLE=value`
  strings, and its lower-level `sd_journal_sendv()` and
  `journal_file_append_entry()` APIs use `struct iovec` arrays whose payloads
  are full fields, not separate key/value structs.
- systemd's file writer scans each full DATA payload for `=` before creating
  and linking the FIELD object.
- systemd sorts entry DATA references by on-disk DATA object offset and removes
  duplicate DATA references before writing the ENTRY object. This is not field
  name sorting; repeated field names with different values remain distinct.
- systemd v260.1 does not have the SDK recent DATA payload cache. It hashes the
  full DATA payload and searches the journal DATA hash chain for every append.
- Go's standard writer API is currently structured: `Field{Name, Value}` plus
  `Append([]Field, EntryOptions)`.
- The Rust public facade has a structured `Field { name, value }` type, but the
  high-level Rust `Log` writer currently exposes raw `&[&[u8]]` journal items
  as its primary write API.
- Node.js and Python writer paths accept structured objects/dicts with `name`
  and `value`, but dynamic-language validation and documentation still need to
  make that the primary API contract rather than arbitrary text input.
- SOW-0023 previously recommended supporting a raw item append path for
  high-throughput encoders. The refined API hierarchy is now an open product
  decision because the systemd-compatible low-level API is raw full payloads,
  while the user also requested a similar structured API across languages.

Inferences:

- The project needs a durable drift matrix, not only tests. Tests show whether a
  behavior passes current fixtures; a matrix explains whether a difference is
  intentional, accidental, performance-motivated, compatibility-required, or
  language-specific.
- Rust should be audited against systemd first, because using Rust as a
  reference before its own differences are classified can copy accidental drift
  into Go, Node.js, and Python.
- Go should be audited against Rust next because it is the next most important
  implementation for Netdata SNMP traps and because known reader/writer strategy
  differences are already visible.
- Cross-language API parity must be audited alongside implementation drift,
  because copying Rust internals blindly may preserve a raw `KEY=value`-only API
  where a structured convenience layer is needed, while copying Go blindly may
  hide the systemd-compatible raw-payload layer.

Unknowns:

- Which Rust/systemd differences are harmless implementation details and which
  affect correctness, live-reader behavior, performance, retention, verification,
  or future compatibility.
- Which Go/Rust differences are accidental versus legitimate language/runtime
  choices.
- Whether Node.js and Python should be audited in this SOW or represented by a
  follow-up SOW after Rust and Go are classified.
- Whether the cross-language standard should be a dual-layer API:
  systemd-compatible raw full-field payloads as the low-level fast path, plus
  structured key/value fields as the higher-level convenience API.
- Whether the structured layer should include an explicit trusted mode for
  callers that guarantee no duplicate full payloads, and whether preserving
  input order should be allowed as a measured non-byte-identity performance
  option.
- Whether Go's recent DATA cache is also unhelpful under the production-shaped
  writer-core workload. Rust evidence says the analogous cache should be
  removed there, but Go needs its own controlled measurement because its hashing,
  mmap strategy, and payload construction differ.

### Acceptance Criteria

- Produce a Rust-versus-systemd difference matrix for journal writer, reader,
  directory, verification, retention, mmap/publication, compression, compact,
  FSS, and journalctl/file-backed behavior.
- Produce a Go-versus-Rust difference matrix for the same surfaces where Go has
  implementation.
- Produce a public writer API matrix for Rust, Go, Node.js, and Python,
  including low-level single-file writer, high-level directory writer, timestamp
  options, binary fields, remapping behavior, and text/raw conversion helpers.
- Define the intended standard API contract, including which layer is canonical
  for systemd-compatible low-level writing and which layer is the higher-level
  structured convenience API. The decision must account for binary values,
  zero-copy/scatter-gather opportunities, systemd parity, and cross-language
  ergonomics.
- Define whether trusted structured append options are part of the public API.
  Any such option must state the exact invariant being delegated to the caller,
  including duplicate full-payload handling and entry-item ordering.
- Classify every important difference as one of:
  - intentional and compatible;
  - intentional performance option candidate;
  - accidental drift to fix;
  - language/runtime-specific with accepted risk;
  - unsupported/out of scope with evidence.
- For every difference, record evidence with file paths and source references.
- For every accidental drift, either fix it in this SOW after user-approved
  classification or map it to a concrete follow-up SOW before closing.
- Remove the Rust recent DATA cache if evidence shows it is an accidental
  performance optimization that does not improve throughput.
- Benchmark Go with the cache enabled and with cache lookup/insertion fully
  removed from the hot path before deciding whether to remove the Go cache.
- Update specs and project skills if the audit changes the durable reference
  model or compatibility workflow.
- Do not make changes outside this repository.

## Analysis

Sources checked:

- `AGENTS.md`
- `.agents/skills/project-journal-compatibility/SKILL.md`
- `.agents/sow/specs/product-scope.md`
- `.agents/sow/current/SOW-0009-20260523-benchmark-profile-optimize.md`
- `.agents/sow/pending/SOW-0036-20260527-live-publication-modes-and-fast-consumers.md`
- `rust/src/crates/journal-core/src/file/mmap.rs`
- `rust/src/crates/journal-core/src/file/file.rs`
- `go/journal/mmap_unix.go`
- `go/journal/writer.go`
- `go/journal/reader.go`
- `go/journal/writer.go`
- `go/journal/log.go`
- `node/src/index.js`
- `node/src/lib/writer.js`
- `node/src/lib/directory-writer.js`
- `python/journal/writer.py`
- `python/journal/directory_writer.py`
- `rust/src/journal/src/lib.rs`
- `rust/src/crates/journal-log-writer/src/log/mod.rs`

Open-source reference evidence:

- `systemd/systemd @ c0a5a2516d28`
  - `src/libsystemd/sd-journal/mmap-cache.c`
  - `src/libsystemd/sd-journal/journal-file.c`
  - `src/libsystemd/sd-journal/sd-journal.c`
  - `src/journal/journald-manager.c`

Current state:

- Rust is already closer to systemd than Go in mmap/object access strategy.
- Go has known deliberate or accidental differences that need classification,
  including writer whole-file mmap and reader `ReadAt()` buffers.
- Public writer API shape currently differs by layer/language. Go's standard
  writer path is structured, Rust has a structured facade but raw high-level
  `Log` write methods, and Node.js/Python use dynamic structured field objects.
- SOW-0036 tracks measurement candidates for publication modes and mmap strategy,
  but it does not cover the full reference-drift question across all journal
  surfaces.
- Existing tests are strong, but they do not themselves explain whether every
  implementation difference is conscious.

Risks:

- Treating Rust as reference without auditing Rust against systemd can
  standardize accidental Rust drift across all other languages.
- Treating systemd as the only reference without preserving Netdata-compatible
  Rust behavior can break existing Netdata integration expectations.
- Overcorrecting implementation details to systemd can reduce maintainability or
  performance without changing file compatibility.
- Underclassifying differences can leave future implementers unsure whether to
  copy systemd, Rust, Go, or an older Netdata behavior.
- Leaving raw/text append as a standard API encourages avoidable parsing,
  copying, and inconsistent binary-field behavior across languages.

## Pre-Implementation Gate

Status: ready for analysis; implementation fixes require classification evidence
and, when behavior/API changes are involved, user decision before code changes.

Problem / root-cause model:

- The project now has multiple implementations and many compatibility passes.
  Without a drift ledger, important differences can become invisible because
  tests pass for current fixtures.
- Rust is the natural project reference, but only after its differences from
  systemd are explicitly classified.
- Go is the next priority consumer implementation, so Go/Rust drift must be
  audited before optimizing or extending Go behavior.
- Rust can be the implementation reference without forcing its current raw
  high-level append API to become the public API reference. The API reference
  must be structured and binary-safe across all languages.

Evidence reviewed:

- `systemd/systemd @ c0a5a2516d28`
  - `src/libsystemd/sd-journal/mmap-cache.c`: systemd windowed mmap cache.
  - `src/libsystemd/sd-journal/journal-file.c`: journal file allocation,
    object movement, post-change notification, verification.
  - `src/libsystemd/sd-journal/journal-file.c:2527`: internal file writer
    entry append API takes `const struct iovec iovec[]`.
  - `src/libsystemd/sd-journal/journal-file.c:2604`: file writer passes each
    iovec as a full DATA payload.
  - `src/libsystemd/sd-journal/journal-file.c:1873`: file writer scans the
    full DATA payload for `=`.
  - `src/libsystemd/sd-journal/journal-file.c:1912`: file writer creates the
    FIELD object from the prefix before `=`.
  - `src/libsystemd/sd-journal/journal-file.c:2630`: file writer sorts entry
    items by DATA object offset.
  - `src/libsystemd/sd-journal/journal-file.c:2631`: file writer removes
    duplicate entry items.
  - `man/sd_journal_print.xml:145`: `sd_journal_send()` is documented around
    `VARIABLE=value` strings.
  - `man/sd_journal_print.xml:156`: `sd_journal_sendv()` is documented as
    `struct iovec` entries, each referencing one field.
  - `src/libsystemd/sd-journal/sd-journal.c`: reader mmap cache ownership.
  - `src/journal/journald-manager.c`: writer mmap cache ownership.
- `rust/src/crates/journal-core/src/file/mmap.rs`: Rust window manager.
- `rust/src/crates/journal-core/src/file/file.rs`: Rust journal file
  reader/writer mmap setup and options.
- `go/journal/mmap_unix.go`: Go Unix writer whole allocated-file mmap.
- `go/journal/reader.go`: Go `ReadAt()` reader paths.
- `go/journal/writer.go`: Go writer publication and arena management.
- `go/journal/writer.go:78`: Go `Field` has `Name string` and `Value []byte`.
- `go/journal/writer.go:267`: Go low-level `Append` takes `[]Field`.
- `go/journal/log.go:414`: Go high-level `Log.Append` takes `[]Field`.
- `node/src/index.js:82`: Node.js exposes `stringField(name, value)`.
- `node/src/index.js:87`: Node.js exposes `binaryField(name, value)`.
- `node/src/lib/writer.js:256`: Node.js low-level `append` takes field objects.
- `node/src/lib/writer.js:271`: Node.js expects each field to have `name` and
  `value`.
- `python/journal/writer.py:269`: Python low-level `append` takes `fields`.
- `python/journal/writer.py:288`: Python expects each field dict to contain
  `name` and `value`.
- `python/journal/directory_writer.py:240`: Python high-level `Log.append`
  takes `fields`.
- `rust/src/journal/src/lib.rs:89`: Rust public facade defines structured
  `Field { name, value }`.
- `rust/src/crates/journal-log-writer/src/log/mod.rs:664`: Rust high-level
  `Log::write_entry` currently takes raw `&[&[u8]]`.
- `rust/src/crates/journal-log-writer/src/log/mod.rs:683`: Rust high-level
  timestamped write path also takes raw `&[&[u8]]`.
- `.agents/sow/pending/SOW-0036-20260527-live-publication-modes-and-fast-consumers.md`:
  existing mmap/publication measurement candidates.

Affected contracts and surfaces:

- Rust SDK reader/writer behavior and compatibility claims.
- Go SDK reader/writer behavior and compatibility claims.
- Future Node.js and Python reference behavior.
- Cross-language public writer API shape and documentation.
- Structured field helper/conversion APIs.
- Product scope spec.
- Project journal compatibility skill.
- Benchmarks and conformance matrix interpretation.
- Netdata integration guidance.

Existing patterns to reuse:

- Existing SOW compatibility gates and matrix terminology.
- Existing structural oracle and interoperability matrix organization.
- Existing product-scope distinction between systemd file compatibility and
  Netdata-compatible SDK naming/API behavior.
- Existing SOW-0036 rule: measure performance candidates before committing
  public API.
- Existing Go `Field` model and Node/Python `{name, value}` field objects as
  evidence for the structured convenience layer.
- Rust public facade `Field` type as evidence that Rust already has a structured
  representation available, even if the high-level `Log` writer still exposes
  raw items.
- systemd `struct iovec` append model as evidence for a raw full-payload
  low-level layer.
- Existing systemd and Rust entry-item normalization as evidence that any
  trusted skip-sort/skip-dedup API must be an explicit compatibility/performance
  mode, not an implicit side effect of structured fields.

Risk and blast radius:

- Medium-to-high analysis risk because the audit spans many journal surfaces.
- Code-change risk is deferred until differences are classified.
- Compatibility risk if a difference is misclassified as implementation detail
  when it affects live readers, corrupt-file handling, FSS, or verification.
- Performance risk if systemd parity is applied mechanically where Rust has a
  conscious faster compatible implementation.
- API regression risk if Go/Node/Python copy Rust high-level raw append methods
  instead of converging on structured field records.
- Performance risk if text `KEY=value` parsing becomes mandatory in hot paths;
  the standard API should avoid parsing by accepting already structured binary
  field values.

Sensitive data handling plan:

- Use only source paths, synthetic fixtures, benchmark summaries, and upstream
  source references. Do not write raw secrets, SNMP communities, customer names,
  personal data, non-private customer-identifying IPs, private endpoints, or
  production logs into durable artifacts.

Implementation plan:

1. Build a Rust-versus-systemd audit table across writer, reader, mmap,
   publication, allocation/retention, compression, compact, FSS, verify,
   directory, and journalctl/file-backed behavior.
2. Classify each Rust/systemd difference and identify required fixes, accepted
   differences, or measurement candidates.
3. Build a Go-versus-Rust audit table for implemented Go surfaces.
4. Classify each Go/Rust difference and identify required fixes, accepted
   differences, or measurement candidates.
5. Build the cross-language writer API matrix and mark any API shape drift,
   especially places where raw full-payload and structured field layers are
   conflated.
6. Present any behavior/API decisions to the user with evidence before code
   changes.
7. Update specs/project skills with the durable reference hierarchy:
   systemd as external file-format authority, Rust as project reference after
   classified differences, other languages aligned to Rust unless explicitly
   accepted, and the chosen cross-language writer API hierarchy.
8. If fixes are small and user decisions are recorded, implement them in this
   SOW; otherwise map each fix to concrete follow-up SOWs.

Validation plan:

- Run same-failure searches for each difference class.
- Run existing matrices relevant to any fixed behavior.
- Validate API examples and tests match the selected API hierarchy. Raw
  full-payload tests should cover systemd-compatible low-level behavior, and
  structured-field tests should cover the higher-level convenience API if kept.
- Validate any trusted structured fast path against stock readers, all SDK
  readers, verifier behavior, live concurrency, query results, and benchmark
  reporting. Exclude it from byte-identity claims unless it still preserves
  systemd-equivalent entry-item ordering and duplicate elimination.
- Run `.agents/sow/audit.sh`.
- Use read-only external reviewers after any implementation change.
- Require every unresolved difference to be classified and mapped before close.

Artifact impact plan:

- AGENTS.md: likely unchanged unless the reference hierarchy needs a
  project-wide guardrail.
- Runtime project skills: likely update
  `.agents/skills/project-journal-compatibility/SKILL.md`.
- Specs: likely update `.agents/sow/specs/product-scope.md`.
- End-user/operator docs: likely update API docs if structured writer methods
  are added or raw methods are reclassified as helpers.
- End-user/operator skills: no current output/reference skills affected.
- SOW lifecycle: new pending SOW; do not close until all differences are
  classified, fixed, rejected, or mapped.
- SOW-status.md: update to record this pending SOW.

Open decisions:

1. Audit scope.
   - Option A: Rust versus systemd only.
   - Option B: Rust versus systemd, then Go versus Rust.
   - Option C: Rust versus systemd, then Go/Node.js/Python versus Rust in one
     large audit.
   - Decision: Option B. The user specifically named Rust/systemd first and
     Go/Rust next, with Node.js and Python later. This keeps the SOW focused on
     the highest-priority implementations.

2. Fix policy after classification.
   - Option A: only produce the matrix, no fixes.
   - Option B: fix accidental drift discovered in Rust or Go when the fix is
     small, local, and does not need a new product decision; otherwise create
     follow-up SOWs.
   - Option C: fix all drift in this SOW.
   - Recommendation: Option B. It avoids leaving obvious debt while preventing a
     broad audit from turning into an unbounded implementation SOW.

3. Standard public writer API shape.
   - Option A: keep each language's current append shape and document the
     differences.
   - Option B: expose a dual-layer API in every language: systemd-compatible
     raw full-field `KEY=value` payloads as the low-level fast path, plus
     structured binary-safe `{name, value}` fields as the higher-level
     convenience API.
   - Option C: make structured binary-safe fields the only standard public
     append API and keep raw `KEY=value` only as internal plumbing.
   - Option D: make raw `KEY=value` byte payloads the only standard append API.
   - Recommendation: Option B. It matches systemd's actual lower-level writer
     model while preserving the user's requirement for a similar structured API
     across languages.

4. Trusted structured fast-path options.
   - Option A: no public trusted mode; always sort and deduplicate entry items
     like systemd.
   - Option B: expose a trusted unique-fields mode that skips duplicate DATA
     elimination only when the caller guarantees no duplicate full payloads.
     Keep offset sorting unless the writer detects the offsets are already
     sorted.
   - Option C: expose both trusted unique-fields and preserve-input-order modes,
     with preserve-input-order documented as a non-byte-identity performance
     option that still must pass reader/verifier/live compatibility.
   - Recommendation: Option B first, measured before any public API commitment.
     It captures the likely Netdata benefit without giving up systemd-like entry
     item ordering. Option C should remain a measured candidate only if sorting
     shows meaningful cost.

## Implications And Decisions

- Decision 1: Option B. Audit Rust against systemd first, then Go against Rust.
- Decision 2: Option B. Fix accidental drift discovered in Rust or Go when the
  fix is small, local, and does not need a new product decision; otherwise
  create follow-up SOWs.
- Decision 3: Option B. Expose a dual-layer writer API in Rust first:
  systemd-compatible raw full-field `KEY=value` payloads as the low-level fast
  path, plus structured binary-safe `{name, value}` fields as the higher-level
  SDK hot path.
- Decision 4: Option B for the first implementation pass. Add or measure a
  trusted unique-fields structured mode that can skip duplicate DATA
  elimination when the caller guarantees no duplicate full payloads, while
  preserving systemd-style offset sorting unless a later measured decision
  chooses a non-byte-identity preserve-input-order mode.
- Sequencing decision: SOW-0009 broad performance work is paused. Complete Rust
  parity/API/benchmark work first, then use Rust as the reference for Go and the
  other languages.

## Plan

1. Audit Rust against systemd and classify differences.
2. Audit Go against Rust and classify differences.
3. Audit public writer API shape across Rust, Go, Node.js, and Python, including
   both systemd-compatible raw full-payload and structured-field layers.
4. Audit trusted structured fast-path options and separate skip-dedup from
   skip-sort because they have different correctness and byte-identity
   implications.
5. Present decisions for differences that affect public behavior, performance,
   or compatibility claims.
6. Fix only approved/local accidental drift or map to follow-up SOWs.
7. Update specs/skills and close after validation.

## Delegation Plan

Implementer:

- Local implementation only unless the user explicitly re-enables external
  implementer agents.

Reviewers:

- Use read-only reviewer agents from the approved pool if this SOW includes code,
  spec, or skill changes beyond the initial SOW creation. Skip
  `llm-netdata-cloud/mimo-v2.5-pro` while unavailable.

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

- Record reviewer or validation failures in this SOW.
- Do not close with unclassified differences.
- Do not close with generic deferred items; map every fix to implemented,
  rejected, or a concrete follow-up SOW.
- Do not close if `.agents/sow/audit.sh` fails.

## Execution Log

### 2026-05-27

- Created this pending SOW at the user's request.
- Recorded Rust/systemd first and Go/Rust second as the audit order.
- Recorded that Node.js and Python alignment should follow after Rust and Go are
  classified.
- Added the user's cross-language API requirement and then recorded the systemd
  v260.1 counter-evidence that the actual systemd low-level writer API is raw
  full `KEY=value` field payloads in `struct iovec` arrays.
- Recorded current API evidence showing Go's structured `[]Field`, Rust's
  structured public facade plus raw high-level `Log`, and Node.js/Python
  structured dynamic field objects.
- Reopened the API hierarchy decision with a recommendation for a dual-layer
  API: raw full-payload low-level plus structured convenience layer.
- Recorded the trusted structured fast-path idea, including the important
  distinction between skipping duplicate DATA elimination and skipping
  systemd-style offset sorting.
- Activated this SOW after the user agreed to pause broad benchmarking and make
  Rust the reference first. Recorded the accepted sequence: Rust parity with
  systemd, Rust dual-layer raw/structured writer API, Rust raw-vs-structured
  performance retest, then Go and other languages.
- Implemented the first Rust reference slice:
  - Added `PayloadParts` so the writer can address raw payloads and structured
    `{name, value}` payloads without forcing a contiguous `KEY=value` buffer in
    the uncompressed hot path.
  - Added Rust low-level writer APIs for raw full-payload fields, structured
    fields, mixed `EntryField` iterators, and `EntryWriteOptions`.
  - Preserved systemd-style DATA offset sorting by default and made
    trusted unique-payload mode skip only duplicate DATA reference elimination.
  - Added high-level Rust `Log` structured write methods that preserve existing
    rotation, retention, timestamp, and remapping behavior.
  - Updated the Rust writer-core benchmark driver and Python benchmark harness
    to record and select Rust `raw-payload` versus `structured-field` API
    modes.
- Addressed first-round reviewer findings for the Rust slice:
  - Added public Rust API documentation for `EntryWriteOptions` and the
    `trusted_unique_payloads` caller invariant.
  - Added direct structured `PayloadParts::equals_slice` coverage.
  - Added mixed raw-plus-structured `EntryField` byte-identity coverage.
  - Added structured duplicate DATA reference coverage proving default
    deduplication and documenting trusted-mode duplicate preservation when the
    caller violates the uniqueness contract.
  - Added a 512-row Rust unit corpus proving raw-payload and structured-field
    writer paths produce byte-identical files.
  - Added an optional 100,000-row benchmark-harness API-mode byte-identity
    check for Rust.
- Addressed second/final review cleanup:
  - Removed module-wide unused/dead-code suppression from the Rust writer and
    cleaned the resulting unused imports/dead helper warnings.
  - Added public documentation for high-level Rust `Log::write_fields*`
    methods.
  - Clarified that `trusted_unique_payloads` never skips DATA-offset sorting.
  - Replaced the safe-but-brittle raw remapping `unwrap()` with slicing from the
    already-validated field-name length.
  - Documented that internal remapping metadata entries use normalized default
    write options while caller fast-path options apply to the user entry.
  - Removed the unused tracing import suppression from the high-level Rust log
    writer.
- Fixed an accidental Rust/systemd drift in `journal-core` Jenkins lookup3
  hashing: empty payloads now return `0xdeadbeefdeadbeef`, matching systemd
  `jenkins_hashlittle2()` and the Netdata vendored `jf` behavior.
- Replaced the temporary multipart Jenkins allocation workaround with an
  allocation-free multipart lookup3 implementation that matches all tested
  contiguous payload splits.
- Updated durable product scope and the journal compatibility project skill
  with the dual-layer writer API hierarchy, trusted unique-payload invariant,
  and Jenkins empty-hash rule.
- Resumed this SOW for the recent DATA cache drift/performance slice after the
  user challenged the Rust/Go cache difference and cache usefulness.
- Verified systemd v260.1 has no equivalent recent DATA payload cache. systemd
  hashes each full DATA payload and searches the journal DATA hash chain in
  `journal_file_append_data()`.
- Removed the Rust recent DATA cache from `journal-core` after controlled
  benchmarks showed it did not improve throughput despite high hit ratios. The
  Rust writer now goes directly through the journal DATA hash/search/create
  path, matching systemd more closely.
- Measured Go's analogous recent DATA cache with an instrumented cache binary
  and a separate no-cache binary that removed all cache lookup and insertion
  calls from `addData()`. Go source was restored after the temporary benchmark
  binaries were built.
- Go corrected benchmark evidence, `100000` rows, `32` fields/row, compact
  format, no compression, no FSS, fixed `134217728` byte max size, pinned to
  CPU `3`, non-instrumented perf binaries:
  - `live_publish_every_entries=0`: cache median `51983.538 rows/sec`,
    no-cache median `52865.162 rows/sec`, no-cache `+1.70%`.
  - `live_publish_every_entries=1`: cache median `47038.782 rows/sec`,
    no-cache median `46890.857 rows/sec`, no-cache `-0.31%`.
  - Instrumented Go cache hit ratio was `64.34%`
    (`2058913` hits / `3200000` lookups) with `65536` slots.
- Benchmark evidence:
  `.local/benchmarks/sow37-go-recent-data-cache-effect-20260528T072101Z/`.
- Removed the Go recent DATA cache from `go/journal/writer.go` after the user
  decision to remove it. The Go writer now uses the normal DATA hash
  lookup/create path for every DATA payload, matching the Rust removal and the
  systemd behavior more closely. The FIELD cache remains unchanged.

## Validation

Acceptance criteria evidence:

- Rust/systemd API evidence recorded:
  - `systemd/systemd @ c0a5a2516d28`
    `src/libsystemd/sd-journal/journal-file.c:2527`: internal append API takes
    `const struct iovec iovec[]`.
  - `systemd/systemd @ c0a5a2516d28`
    `src/libsystemd/sd-journal/journal-file.c:2604`: each iovec is used as a
    full DATA payload.
  - `systemd/systemd @ c0a5a2516d28`
    `src/libsystemd/sd-journal/journal-file.c:2630`: ENTRY DATA references are
    sorted by DATA object offset.
  - `systemd/systemd @ c0a5a2516d28`
    `src/libsystemd/sd-journal/journal-file.c:2631`: duplicate ENTRY DATA
    references are removed.
- Rust dual-layer writer API is implemented in:
  - `rust/src/crates/journal-core/src/file/writer.rs`
  - `rust/src/crates/journal-log-writer/src/log/mod.rs`
  - `rust/src/internal/testcmd/writer_core_bench/src/main.rs`
- Structured writer compatibility evidence:
  - Rust test `structured_writer_matches_raw_payload_writer_bytes` proves raw
    and structured append paths produce byte-identical uncompressed files for
    the same entry when header identity is fixed.
  - Rust test `mixed_entry_fields_match_raw_payload_writer_bytes` proves the
    public mixed raw-plus-structured `EntryField` API produces byte-identical
    output to the equivalent all-raw append path.
  - Rust test `structured_writer_preserves_binary_field_values` proves
    structured values preserve binary bytes including NUL and `=`.
  - Rust test `structured_writer_deduplicates_duplicate_payloads_by_default`
    proves structured entries remove duplicate DATA references by default and
    preserve duplicates only when `trusted_unique_payloads=true`, documenting
    the caller-contract violation case.
  - Rust test `trusted_unique_payloads_keeps_unique_entry_output_identical`
    proves trusted unique-payload mode does not change output when the caller's
    uniqueness contract is met.
  - Rust test
    `structured_writer_matches_raw_payload_writer_bytes_across_deterministic_corpus`
    proves raw and structured append paths produce byte-identical output across
    512 deterministic rows with fixed, low-cardinality, medium-cardinality,
    high-cardinality, empty, and binary values.
  - Rust test `payload_parts_structured_equals_contiguous_payload` proves
    structured multi-part payload comparison matches the equivalent contiguous
    `KEY=value` bytes, including binary values containing `=`.
- Rust recent DATA cache removal evidence:
  - Rust cache benchmark with `4096` slots: `46.18%` hit ratio but no material
    throughput gain.
  - Rust cache benchmark with `65536` slots: `64.67%` hit ratio but lower
    throughput than the no-cache variant.
  - Rust no-cache writer path removes cache lookup, cache insertion, cache slot
    hashing, cache payload comparison, and cache payload copy from the DATA hot
    path.
- Go recent DATA cache benchmark evidence:
  - Go cache with `65536` slots reached `64.34%` hit ratio.
  - Go no-cache was `+1.70%` faster at `live_publish_every_entries=0` and
    `0.31%` slower at `live_publish_every_entries=1`, which is effectively
    noise-level compared with run variance.
  - The corrected Go no-cache perf binary removed all `dataCache.get()` and
    `dataCache.insert()` calls from `addData()`.
  - Production Go code now removes the `dataCache` writer field, the
    `recentDataCache` types, the recent DATA cache constants, and all
    `dataCache.get()` / `dataCache.insert()` calls from `addData()`.
- Jenkins drift evidence:
  - `rust/src/crates/jf/journal_file/src/hash.rs` already treated empty
    Jenkins lookup3 as `0xdeadbeefdeadbeef`.
  - `systemd/systemd @ c0a5a2516d28`
    `src/libsystemd/sd-journal/lookup3.h:14` and
    `src/libsystemd/sd-journal/lookup3.c:470`: `jenkins_hashlittle2()` starts
    both returned halves from `0xdeadbeef` for zero-length input.

Tests or equivalent validation:

- `cargo test --manifest-path rust/Cargo.toml -p journal-core`
  - Result after final cleanup: pass. 55 unit tests passed; 1 doc test passed;
    3 doc tests ignored. No warnings in output.
- `cargo test --manifest-path rust/Cargo.toml -p journal-log-writer`
  - Result after final cleanup: pass. 48 tests passed; 1 doc test passed.
- `cargo check --manifest-path rust/Cargo.toml -p writer_core_bench`
  - Result after final cleanup: pass.
- `cargo check --manifest-path rust/Cargo.toml -p journal-core`
  - Result after final cleanup: pass. No warnings in output.
- `cargo fmt --all --check`
  - Result after final cleanup: pass.
- `python3 -m py_compile tests/benchmarks/run_writer_core_benchmarks.py`
  - Result: pass.
- `git diff --check`
  - Result after final cleanup: pass.
- `.agents/sow/audit.sh`
  - Result after final cleanup: pass.
- `CARGO_HOME=.local/cargo-home CARGO_TARGET_DIR=.local/cargo-target cargo test -p journal-core --manifest-path rust/Cargo.toml`
  - Result after removing the Rust recent DATA cache: pass. 56 journal-core
    tests passed, 1 doc-test passed, and 3 doc-tests were ignored. The two
    removed tests were cache-specific unit tests for code that no longer
    exists.
- `GOCACHE=$PWD/../.local/go-cache GOMODCACHE=$PWD/../.local/go-mod-cache GOPATH=$PWD/../.local/go-path go test ./...`
  from `go/`
  - Result after removing the Go recent DATA cache: pass. Packages tested:
    `adapter`, `cmd/journalctl`, and `journal`; testcmd packages had no test
    files.
- Writer benchmark smoke, 1,000 rows, compact, no compression, no FSS,
  `final_state=online`, fixed `max_size_bytes=134217728`:
  - Rust raw-payload: pass, stock `journalctl --verify --file` pass, median
    append rate 68.6k rows/s for one measured repetition.
  - Rust structured-field with trusted unique-payloads: pass, stock
    `journalctl --verify --file` pass, median append rate 63.5k rows/s for one
    measured repetition.
- Writer benchmark after Jenkins correction, 100,000 rows, 1 warmup, 3
  measured repetitions, compact, no compression, no FSS, `final_state=online`,
  fixed `max_size_bytes=134217728`, data buckets 233016, field buckets 1023:
  - systemd raw-payload median append rate: 35.1k rows/s.
  - Rust raw-payload median append rate: 45.6k rows/s, 1.30x systemd median.
  - systemd raw-payload median append rate in the structured comparison run:
    32.8k rows/s.
  - Rust structured-field with trusted unique-payloads median append rate:
    45.1k rows/s, 1.38x that run's systemd median.
  - Rust structured-field is effectively at Rust raw-payload speed for this
    dataset after removing the multipart Jenkins allocation workaround.
- Rust API-mode byte-identity validation after first-round reviewer fixes,
  100,000 rows, compact, no compression, no FSS, `final_state=online`,
  fixed `max_size_bytes=134217728`, trusted unique-payloads enabled:
  - Report:
    `.local/benchmarks/writer-core/compact-none-fss-off-rust-structured-field-trusted-unique-20260527T213216369369Z/report.json`.
  - Result: pass.
  - Raw-payload append rate in the comparison run: 46.7k rows/s.
  - Structured-field append rate in the comparison run: 47.2k rows/s.
  - Raw-payload and structured-field output size: 134217728 bytes.
  - Raw-payload and structured-field SHA-256:
    `34af8ed46128b8089b6f8d070c53983ca8c593dad7d568be8317cc32348deeac`.
  - Stock `journalctl --verify --file` passed for both comparison outputs.

Real-use evidence:

- Stock `journalctl --verify --file` was run by the benchmark harness for each
  measured Rust and systemd output file in the 1,000-row and 100,000-row runs.
  All measured runs passed verification.

Reviewer findings:

- First review round:
  - GLM: production-grade, no blocking findings. Disposition: recorded
    non-blocking observations about sortedness scan cost, bounded cache
    allocations, and keyed-hash fallback as SOW-0009/SOW-0036 follow-up context.
  - Qwen: production-grade, no blocking findings. Disposition: recorded
    remapping allocation and field-cache clear behavior as non-blocking.
  - Minimax: production-grade after adding public documentation for
    `EntryWriteOptions::trusted_unique_payloads`. Disposition: API docs added.
  - Kimi: not production-grade before adding structured duplicate coverage,
    mixed `EntryField` coverage, and raw-versus-structured corpus byte-identity
    validation. Disposition: all three coverage gaps were implemented; live
    reader stress remains explicitly tracked by SOW-0036/SOW-0009, not closed
    by this Rust API slice.
- Second review round:
  - GLM: production-grade; no blocking findings. Disposition: accepted
    non-blocking observations as SOW-0009/SOW-0036 follow-up context.
  - Qwen: production-grade; no blocking findings. Disposition: accepted
    remapping-path allocation and cache observations as performance follow-up
    context.
  - Minimax: production-grade; requested clarifying that
    `trusted_unique_payloads` preserves offset sorting. Disposition:
    documentation updated.
  - Kimi: production-grade; requested high-level structured API docs and
    removing the brittle raw-remapping `unwrap()`. Disposition: both fixed.
- Final review round after cleanup:
  - Minimax: production-grade; one low documentation clarification requested.
    Disposition: `trusted_unique_payloads` docs now explicitly state offset
    sorting is always performed.
  - Qwen: production-grade; non-blocking performance observations only.
    Disposition: tracked as SOW-0009/SOW-0036 performance context.
  - GLM: production-grade; only low cleanup observations. Disposition: removed
    unused tracing import suppression; remaining cache/FIXME performance or
    unreachable-path observations stay follow-up context.
  - Kimi final rerun stalled after reading files for about ten minutes. The
    exact `timeout`/`opencode` PIDs for that stalled reviewer were terminated;
    Kimi's prior completed round after the substantive fixes was
    production-grade, and all other final reviewers were production-grade.

Same-failure scan:

- Rust writer API call sites were searched for `add_entry`, `StructuredField`,
  `trusted_unique`, recent data cache, field cache, and benchmark API mode
  usage before adding the Rust API tests and benchmark mode.
- Jenkins hashing was searched across `journal-core`, the Netdata vendored
  `jf` crate, and systemd v260.1. The empty-hash drift was fixed in
  `journal-core` and covered by new expected-value tests.

Sensitive data gate:

- This SOW contains only repository paths, source references, and synthetic
  compatibility/performance context. It does not contain raw secrets, SNMP
  communities, customer identifiers, personal data, private endpoints, or
  production log data.

Artifact maintenance gate:

- AGENTS.md: no update needed for this Rust slice; existing project rules
  already cover the workflow.
- Runtime project skills:
  `.agents/skills/project-journal-compatibility/SKILL.md` updated with the
  dual-layer writer API hierarchy, trusted unique-payload invariant, and
  Jenkins empty-hash rule.
- Specs: `.agents/sow/specs/product-scope.md` updated with the same durable
  writer API and hashing contracts plus the current Rust writer slice.
- End-user/operator docs: pending before SOW close. Public Rust API docs may
  need examples after reviewer pass and before release tagging.
- End-user/operator skills: no update needed.
- SOW lifecycle: SOW-0037 moved to current/in-progress and SOW-0009 paused.
- SOW-status.md: updated to record SOW-0037 in progress and SOW-0009 paused.

Specs update:

- Updated for the Rust dual-layer writer API and Jenkins hashing contract.

Project skills update:

- Updated for future journal compatibility work.

End-user/operator docs update:

- Pending before this SOW closes.

End-user/operator skills update:

- Not affected by SOW creation.

Lessons:

- Multi-language compatibility needs both executable tests and an explicit drift
  ledger. Passing tests alone does not prove differences are intentional.
- API parity is part of drift control. The project should not standardize on
  either raw full-payload or structured fields without explicitly separating
  systemd-compatible low-level behavior from SDK convenience behavior.

Follow-up mapping:

- Go must be audited and adjusted against the Rust reference after the Rust
  review cycle completes.
- Node.js and Python API parity remain part of later alignment work after Rust
  and Go are classified.
- SOW-0036 continues to track measured publication/mmap mode candidates.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and
later testing or use found broken behavior. Use a dated `## Regression -
YYYY-MM-DD` heading at the end of the file. Never prepend regression content
above the original SOW narrative.
