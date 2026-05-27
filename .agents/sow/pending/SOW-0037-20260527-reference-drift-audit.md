# SOW-0037 - Reference Drift Audit

## Status

Status: open

Sub-state: pending analysis; no implementation started.

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
- Decision 2: pending if implementation is started. Recommendation is Option B.
- Decision 3: pending after systemd API evidence. Recommendation is now the
  dual-layer Option B.
- Decision 4: pending. Recommendation is trusted unique-fields first, with
  preserve-input-order measured separately before public commitment.

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

## Validation

Acceptance criteria evidence:

- Pending.

Tests or equivalent validation:

- Pending.

Real-use evidence:

- Pending.

Reviewer findings:

- Pending.

Same-failure scan:

- Pending.

Sensitive data gate:

- This SOW contains only repository paths, source references, and synthetic
  compatibility/performance context. It does not contain raw secrets, SNMP
  communities, customer identifiers, personal data, private endpoints, or
  production log data.

Artifact maintenance gate:

- AGENTS.md: no update needed for SOW creation.
- Runtime project skills: likely future update when reference hierarchy is
  formalized after audit.
- Specs: likely future update when reference hierarchy and accepted differences
  are formalized after audit.
- End-user/operator docs: no update yet; no behavior changed.
- End-user/operator skills: no update needed.
- SOW lifecycle: pending SOW created with `Status: open`.
- SOW-status.md: updated to record this pending SOW.

Specs update:

- Pending future audit outcome.

Project skills update:

- Pending future audit outcome.

End-user/operator docs update:

- Pending future implementation if public behavior changes.

End-user/operator skills update:

- Not affected by SOW creation.

Lessons:

- Multi-language compatibility needs both executable tests and an explicit drift
  ledger. Passing tests alone does not prove differences are intentional.
- API parity is part of drift control. The project should not standardize on
  either raw full-payload or structured fields without explicitly separating
  systemd-compatible low-level behavior from SDK convenience behavior.

Follow-up mapping:

- Pending audit.

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
