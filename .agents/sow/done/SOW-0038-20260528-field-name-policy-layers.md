# SOW-0038 - Field Name Policy Layers

## Status

Status: completed

Sub-state: completed on 2026-05-28 as the critical path for the `v0.3.0`
release needed by SNMP traps integration. The closure commit is the intended
target for the root `v0.3.0` tag and Go module `go/v0.3.0` tag.

User correction on 2026-05-28: producer-specific mapped fields and mapping
markers are not SDK behavior. They must be removed from the SDK code, public
API, tests, specs, and documentation.

## Requirements

### Purpose

Make the SDK field-name behavior fit the full intended product surface:

- future low-level journal-file tools and replicas need all freedom the on-disk
  journal structures allow;
- journald-like producers need trusted systemd fields such as `_HOSTNAME`;
- untrusted application logging compatibility needs journald's app-facing field
  restrictions;
- producers may layer their own naming rules before calling the SDK, but the SDK
  itself must stay producer-neutral.

This matters immediately because the SNMP traps Go integration wants to emit
systemd facet fields such as `_HOSTNAME`, and `v0.2.0` high-level `Log.Append`
currently remaps user-provided protected fields.

### User Request

Create a SOW that records the field-name policy model, analyzes current SDK
behavior, analyzes what journal structures allow, analyzes what systemd does for
journald and application inputs, and proposes a plan to fix the SDK and
documentation.

The user then requested implementing the policy model for all languages and
modes, testing it, and releasing `v0.3.0` so SNMP traps integration can consume
the corrected Go API.

### Assistant Understanding

Facts:

- The SDK needs three base field-name policy levels:
  - `RAW`: what the journal file structures can encode. This mode produces
    journal files, but does not guarantee systemd ecosystem compatibility.
  - `JOURNALD`: what journald itself can write as a trusted producer. This mode
    must remain stock systemd-friendly and preserve protected fields.
  - `JOURNAL-APP`: what untrusted applications logging to journald can submit.
    This mode must enforce journald's app-facing restrictions.
- RAW still has mandatory structural rules:
  - each DATA payload is `FIELD=value`;
  - the first `=` splits field name from value;
  - a field name cannot contain `=`;
  - the field name before the first `=` cannot be empty;
  - values may contain `=`, NUL, and other binary bytes.
- Producer-specific field-name transformations are outside the SDK field-name
  policy model.
- The current high-level `Log` remapping behavior can break SNMP traps because
  `_HOSTNAME` is remapped instead of being written as a systemd trusted field.

Inferences:

- The current SDK conflated producer-specific convenience with high-level
  journal writer policy.
- The low-level writers currently implement a systemd-like field-name
  validation layer, not true RAW structure capability.
- Fixing this cleanly requires API and docs changes in Rust, Go, Node.js, and
  Python, plus explicit tests for all three policy levels.

Unknowns:

- The desired default field policy for existing high-level constructors must be
  confirmed before implementation because every default has compatibility
  implications.
- The exact `JOURNAL-APP` invalid-field handling must be confirmed: exact
  journald emulation drops invalid fields, while a fail-fast SDK API may be
  easier for operators to diagnose.

### Acceptance Criteria

- Specs define `RAW`, `JOURNALD`, and `JOURNAL-APP` precisely and state that
  producer-specific remapping is outside the SDK.
- Rust, Go, Node.js, and Python expose equivalent field-name policy APIs for
  low-level direct-file writers and high-level directory writers.
- RAW writer paths allow every field name the journal structures can encode,
  subject only to structural constraints such as non-empty name before the first
  `=` and no `=` inside the field name.
- JOURNALD writer paths allow protected `_...` trusted fields and enforce
  stock systemd field-name limits that keep stock `journalctl` and libsystemd
  readers compatible.
- JOURNAL-APP writer paths enforce app-facing journald restrictions, including
  disallowing protected `_...` caller fields.
- No SDK writer silently remaps or rewrites caller field names.
- No SDK writer, reader, indexer, query path, documentation, or public API keeps
  project-specific field remapping behavior.
- Tests cover `_HOSTNAME`, `_TRANSPORT`, dotted/lowercase names, invalid
  symbols, long names, first-`=` splitting, binary values, duplicate fields,
  and cross-language behavior.
- Stock `journalctl --file` / `--directory` and libsystemd compatibility tests
  pass for JOURNALD and JOURNAL-APP generated files.
- RAW-mode files with systemd-incompatible names are validated by SDK readers
  and verifier paths, but are not claimed to pass stock systemd tooling.
- Documentation explains when to choose RAW, JOURNALD, and JOURNAL-APP without
  adding producer-specific SDK modes.

## Analysis

Sources checked:

- `go/journal/field_remap.go`
- `go/journal/log.go`
- `go/journal/writer.go`
- `node/src/lib/field-remap.js`
- `node/src/lib/directory-writer.js`
- `node/src/lib/writer.js`
- `python/journal/field_remap.py`
- `python/journal/directory_writer.py`
- `python/journal/writer.py`
- `rust/src/crates/journal-log-writer/src/log/mod.rs`
- `rust/src/crates/journal-core/src/file/writer.rs`
- `.agents/sow/done/SOW-0023-20260525-netdata-ingestion-writer-api.md`
- `.agents/sow/specs/product-scope.md`
- `systemd/systemd @ c3cd6e5bdb07` tag `v260.1`
- `ktsaou/netdata @ 00305266364e`

Current state:

- Current Go high-level `Log.Append` always calls `remapLogFields()` before
  writing the user entry:
  - `go/journal/log.go:436`
- Current Go high-level remapping treats a name as compatible only when it
  starts with `A-Z`, is at most 64 bytes, and contains only `A-Z`, `0-9`, and
  `_`:
  - `go/journal/field_remap.go:78`
- Therefore `_HOSTNAME` and `_CUSTOM_FIELD` are remapped today because they do
  not start with `A-Z`:
  - `go/journal/field_remap.go:82`
  - `go/journal/field_remap.go:95`
- Current Go low-level `Writer.Append` has different behavior: it allows a
  leading `_` but rejects empty names, names longer than 64, digit-first names,
  and any character outside `A-Z`, `0-9`, `_`:
  - `go/journal/writer.go:599`
- Node.js and Python mirror the same high-level remap behavior:
  - `node/src/lib/field-remap.js:27`
  - `node/src/lib/directory-writer.js:150`
  - `python/journal/field_remap.py:33`
  - `python/journal/directory_writer.py:260`
- Node.js and Python low-level writers also apply the current systemd-like
  validation:
  - `node/src/lib/writer.js:278`
  - `node/src/lib/writer.js:1079`
  - `python/journal/writer.py:306`
  - `python/journal/writer.py:963`
- Rust high-level `Log` also remaps non-compatible names before writing:
  - `rust/src/crates/journal-log-writer/src/log/mod.rs:735`
  - `rust/src/crates/journal-log-writer/src/log/mod.rs:750`
- Rust low-level `journal-core` writes structured and raw entry fields without
  the high-level remapping pass:
  - `rust/src/crates/journal-core/src/file/writer.rs:604`
  - `rust/src/crates/journal-core/src/file/writer.rs:624`
- SOW-0023 is where the remapping became cross-language high-level behavior:
  - `.agents/sow/done/SOW-0023-20260525-netdata-ingestion-writer-api.md:331`
  - `.agents/sow/done/SOW-0023-20260525-netdata-ingestion-writer-api.md:743`

What the journal structures allow:

- DATA payloads are byte payloads and systemd's append path finds the first
  `=` with `memchr(data, '=', size)`.
  - `systemd/systemd @ c3cd6e5bdb07`
    `src/libsystemd/sd-journal/journal-file.c:1873`
- A DATA payload without an `=` is rejected by the systemd writer path.
  - `systemd/systemd @ c3cd6e5bdb07`
    `src/libsystemd/sd-journal/journal-file.c:1874`
- The FIELD object is created from bytes before that first `=`.
  - `systemd/systemd @ c3cd6e5bdb07`
    `src/libsystemd/sd-journal/journal-file.c:1912`
- The on-disk object structures store FIELD and DATA payload bytes; RAW mode
  should be defined from these structures and the mandatory first-`=` split,
  not from systemd's higher-level writer validation.
- Consequence: RAW mode must not allow `=` in field names, because the first
  `=` ends the field name. RAW mode may allow lowercase, symbols, leading `_`,
  long names, and byte-oriented names only to the extent the SDK reader,
  writer, verifier, indexer, and query APIs can represent and preserve them.
  Such files are not guaranteed to be accepted by stock systemd tools.

What systemd does for trusted journald/file writing:

- systemd's `journal_field_valid(..., allow_protected=true)` allows leading
  `_`, disallows empty names, disallows names longer than 64, disallows
  digit-first names, and allows only `A-Z`, `0-9`, and `_`.
  - `systemd/systemd @ c3cd6e5bdb07`
    `src/libsystemd/sd-journal/journal-file.c:1710`
  - `systemd/systemd @ c3cd6e5bdb07`
    `src/libsystemd/sd-journal/journal-file.c:1722`
  - `systemd/systemd @ c3cd6e5bdb07`
    `src/libsystemd/sd-journal/journal-file.c:1726`
  - `systemd/systemd @ c3cd6e5bdb07`
    `src/libsystemd/sd-journal/journal-file.c:1734`
  - `systemd/systemd @ c3cd6e5bdb07`
    `src/libsystemd/sd-journal/journal-file.c:1738`
  - `systemd/systemd @ c3cd6e5bdb07`
    `src/libsystemd/sd-journal/journal-file.c:1764`
- journald writes protected trusted fields such as `_HOSTNAME`, `_PID`, `_UID`,
  and `_SYSTEMD_UNIT`.
  - `systemd/systemd @ c3cd6e5bdb07`
    `src/journal/journald-manager.c:863`
  - `systemd/systemd @ c3cd6e5bdb07`
    `src/journal/journald-manager.c:1088`
  - `systemd/systemd @ c3cd6e5bdb07`
    `src/journal/journald-manager.c:1105`
- Consequence: JOURNALD mode should preserve `_HOSTNAME`, `_TRANSPORT`,
  `_PID`, `_UID`, `_SYSTEMD_UNIT`, and other trusted fields, not remap them.

What systemd does for applications:

- `sd_journal_sendv()` requires each submitted item to contain a non-leading
  `=`, and returns `-EINVAL` for missing or leading `=`.
  - `systemd/systemd @ c3cd6e5bdb07`
    `src/libsystemd/sd-journal/journal-send.c:242`
  - `systemd/systemd @ c3cd6e5bdb07`
    `src/libsystemd/sd-journal/journal-send.c:243`
- The journald native socket path validates app field names with
  `journal_field_valid(..., allow_protected=false)`.
  - `systemd/systemd @ c3cd6e5bdb07`
    `src/journal/journald-native.c:157`
  - `systemd/systemd @ c3cd6e5bdb07`
    `src/journal/journald-native.c:159`
- In app mode, `journal_field_valid(..., allow_protected=false)` rejects
  leading `_`.
  - `systemd/systemd @ c3cd6e5bdb07`
    `src/libsystemd/sd-journal/journal-file.c:1730`
- Consequence: JOURNAL-APP mode should not allow caller-provided `_HOSTNAME`.
  It should enforce the app-facing rules separately from JOURNALD mode.

What Netdata vendored Rust did at import time:

- Netdata's vendored compatibility helper treats names as compatible only when
  they start with uppercase ASCII. It explicitly tests `_MESSAGE` as invalid.
  - `ktsaou/netdata @ 00305266364e`
    `src/crates/journal-core/src/field_map.rs:6`
  - `ktsaou/netdata @ 00305266364e`
    `src/crates/journal-core/src/field_map.rs:11`
  - `ktsaou/netdata @ 00305266364e`
    `src/crates/journal-core/src/field_map.rs:130`
- Netdata's vendored high-level writer remaps incompatible names and emits
  mapping metadata.
  - `ktsaou/netdata @ 00305266364e`
    `src/crates/journal-log-writer/src/log/mod.rs:345`
  - `ktsaou/netdata @ 00305266364e`
    `src/crates/journal-log-writer/src/log/mod.rs:360`
  - `ktsaou/netdata @ 00305266364e`
    `src/crates/journal-log-writer/src/log/mod.rs:367`
- Consequence: v0.2.0 copied a real Netdata Rust behavior, but it was a
  producer-specific behavior and should not be SDK policy.

Risks:

- SNMP traps integration can lose intended stock journalctl/systemd facets if
  `_HOSTNAME` is silently remapped.
- RAW mode can produce files stock systemd tools reject or misinterpret. This
  is acceptable only if RAW is clearly documented and tested as SDK-readable,
  not systemd-friendly.
- Default-policy changes are public API behavior changes after `v0.2.0`.
- Reader/indexer/query code may assume field names are UTF-8 strings or
  systemd-compatible identifiers; RAW byte-oriented names may need API-specific
  representation decisions.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The SDK currently conflates three layers:
  - journal-file structure capability;
  - systemd/journald-compatible trusted writer policy;
  - producer-specific field remapping.
- SOW-0023 generalized a producer-specific Rust remapping layer to all high-level
  `Log` writers in all languages. That made `_HOSTNAME` remap by default,
  which is wrong for journald-like producers and SNMP traps.

Evidence reviewed:

- Current high-level Go remap call:
  - `go/journal/log.go:436`
- Current Go high-level compatibility predicate:
  - `go/journal/field_remap.go:78`
- Current Go low-level field validation:
  - `go/journal/writer.go:599`
- Current Node.js high-level remap call:
  - `node/src/lib/directory-writer.js:150`
- Current Python high-level remap call:
  - `python/journal/directory_writer.py:260`
- Current Rust high-level remap path:
  - `rust/src/crates/journal-log-writer/src/log/mod.rs:735`
- systemd v260.1 field validation:
  - `systemd/systemd @ c3cd6e5bdb07`
    `src/libsystemd/sd-journal/journal-file.c:1710`
- systemd v260.1 app input validation:
  - `systemd/systemd @ c3cd6e5bdb07`
    `src/journal/journald-native.c:159`
- systemd v260.1 DATA payload first-`=` split:
  - `systemd/systemd @ c3cd6e5bdb07`
    `src/libsystemd/sd-journal/journal-file.c:1873`
- Netdata vendored Rust remapping:
  - `ktsaou/netdata @ 00305266364e`
    `src/crates/journal-log-writer/src/log/mod.rs:345`

Affected contracts and surfaces:

- Rust:
  - `journal-core` low-level writer APIs.
  - `journal-log-writer` high-level `Log` APIs.
  - README and tests.
- Go:
  - `journal.Writer`, `journal.Log`, `LogConfig`, `Options`, docs, tests, and
    benchmark/test commands.
- Node.js:
  - direct writer, directory writer, docs, tests.
- Python:
  - direct writer, directory writer, docs, tests.
- Shared:
  - `.agents/sow/specs/product-scope.md`.
  - `.agents/skills/project-journal-compatibility/SKILL.md`.
  - interoperability/conformance tests.
  - release tags if this becomes a patch release.

Existing patterns to reuse:

- Existing low-level writer validators approximate JOURNALD mode.
- Existing shared compatibility harnesses can validate JOURNALD/JOURNAL-APP
  stock-reader compatibility.
- Existing SDK readers can validate RAW files that stock systemd is not
  expected to accept.

Risk and blast radius:

- API behavior change across all four languages.
- Potential Go integration risk for SNMP traps if not fixed quickly.
- Potential migration risk for any consumer that accidentally depended on the
  producer-specific remapping behavior copied into `v0.2.0`.
- Potential reader/indexer risk if RAW byte-oriented names are accepted without
  corresponding reader representation tests.
- Stock compatibility risk if JOURNALD/JOURNAL-APP modes are not tested against
  stock `journalctl` and libsystemd readers.

Sensitive data handling plan:

- Use synthetic field names and values only.
- Do not record real trap payloads, community strings, customer hostnames,
  customer IP addresses, credentials, bearer tokens, or incident data in SOWs,
  tests, docs, fixtures, prompts, or comments.
- Use placeholders such as `_HOSTNAME=synthetic-host` and
  `MESSAGE=synthetic`.

Implementation plan:

1. Update specs and project skill first.
   - Define RAW, JOURNALD, and JOURNAL-APP.
   - Record which modes promise stock systemd compatibility.
2. Add shared field-name policy fixtures.
   - Include `_HOSTNAME`, `_TRANSPORT`, `_PID`, `MESSAGE`, `foo.bar`,
     `log.body.HostName`, `field name`, long names, lowercase, and binary
     values.
3. Add public policy API in each language.
   - Rust enum/config.
   - Go `FieldNamePolicy` or equivalent on `Options` / `LogConfig`.
   - Node.js string/constant option.
   - Python enum/string option.
4. Split policy enforcement from remapping.
   - RAW: structural first-`=` rules and object-size/offset constraints only.
   - JOURNALD: systemd trusted writer field validation, no remap.
   - JOURNAL-APP: journald app-facing validation, no remap.
5. Update readers/indexers only where RAW mode requires byte-oriented field-name
   handling, and remove producer-specific remapping behavior from reader,
   indexer, and query paths.
6. Update docs and examples.
   - Trusted journald-like producers should use JOURNALD when they need fields
     such as `_HOSTNAME`.
   - Applications that want journald app semantics should use JOURNAL-APP.
   - File-format tools and replicas can use RAW when stock systemd
     compatibility is not required.
7. Validate and review.
   - Run language suites.
   - Run stock reader checks for JOURNALD and JOURNAL-APP.
   - Run SDK reader checks for RAW.
   - Run read-only reviewers after a meaningful batch.

Validation plan:

- Unit tests per language:
  - RAW accepts lowercase/symbol/long field names subject to structural rules.
  - RAW rejects missing `=`, empty name before `=`, and `=` inside structured
    field names when represented as name/value.
  - JOURNALD preserves `_HOSTNAME`.
  - JOURNALD rejects lowercase/dotted/symbol/long names.
  - JOURNAL-APP rejects or drops protected `_HOSTNAME` per user decision.
- Interoperability tests:
  - JOURNALD and JOURNAL-APP files pass stock `journalctl --verify --file`
    where applicable and are readable by stock `journalctl --file`.
  - RAW files are read by SDK readers across languages; stock systemd tests are
    not required for incompatible RAW fixtures and must not be claimed.
- Regression tests:
  - SNMP-shaped entry with `_HOSTNAME` remains `_HOSTNAME` in JOURNALD mode.
  - Dotted/lowercase fields are rejected or dropped by systemd-compatible
    policies and accepted only by RAW.

Artifact impact plan:

- AGENTS.md: likely no update unless release/process policy changes.
- Runtime project skills: update `project-journal-compatibility` with the
  three policy levels and RAW stock-compatibility warning.
- Specs: update `product-scope.md`.
- End-user/operator docs: update Rust/Go/Node/Python READMEs and Go API docs.
- End-user/operator skills: no current output/reference skills affected unless
  new integration skills are added later.
- SOW lifecycle: SOW-0038 is active as the corrective implementation SOW for the
  field-name policy and producer-remapping scope error.
- SOW-status.md: update to list this pending SOW.

Open-source reference evidence:

- `systemd/systemd @ c3cd6e5bdb07`
  - `src/libsystemd/sd-journal/journal-file.c:1710`
  - `src/libsystemd/sd-journal/journal-file.c:1764`
  - `src/libsystemd/sd-journal/journal-file.c:1873`
  - `src/libsystemd/sd-journal/journal-file.c:1912`
  - `src/libsystemd/sd-journal/journal-send.c:242`
  - `src/journal/journald-native.c:159`
  - `src/journal/journald-manager.c:863`
  - `src/journal/journald-manager.c:1088`
- `ktsaou/netdata @ 00305266364e`
  - `src/crates/journal-core/src/field_map.rs:6`
  - `src/crates/journal-core/src/field_map.rs:130`
  - `src/crates/journal-log-writer/src/log/mod.rs:345`
  - `src/crates/journal-log-writer/src/log/mod.rs:360`

Resolved decisions:

1. Default policy for existing high-level `Log` constructors.
   - A. Default to JOURNALD.
     - Pros: fixes `_HOSTNAME` for SNMP traps, remains stock systemd-friendly,
       keeps existing easy constructor ergonomics, and matches trusted writer
       capability.
     - Cons: dotted/lowercase fields that worked through implicit remapping in
       `v0.2.0` will fail under systemd-compatible policies unless callers
       choose RAW or transform fields before calling the SDK.
     - Implication: best patch-release path for Netdata SNMP integration.
   - B. Require explicit policy.
     - Pros: highest clarity; every consumer consciously picks RAW, JOURNALD,
       or JOURNAL-APP.
     - Cons: broader breaking change and awkward in languages/configs where
       zero values are expected to work.
     - Implication: best long-term purity, slower integration path.
   - C. Default to JOURNAL-APP.
     - Pros: safest for untrusted app producers.
     - Cons: still breaks `_HOSTNAME` and other journald trusted facets for
       SNMP traps.
     - Implication: not fit for the immediate Netdata use case.
   - D. Keep implicit producer remapping default.
     - Pros: preserves `v0.2.0` behavior.
     - Cons: known wrong behavior for SNMP traps and trusted fields.
     - Implication: rejected for this SOW.
   - Recommendation: A for `v0.2.1`, while examples and docs show consumers
     explicitly setting the policy.
   - Decision: A, updated for the requested `v0.3.0` release. Existing
     high-level constructors default to JOURNALD.

2. JOURNAL-APP invalid/protected field handling.
   - A. Emulate journald exactly enough to drop invalid/protected caller fields
     and write the remaining valid fields.
     - Pros: closest to app logging through journald.
     - Cons: silent drops can surprise SDK callers; needs clear reporting or
       tests.
   - B. Reject the append with an error on the first invalid/protected field.
     - Pros: fail-fast and easier to debug.
     - Cons: not the same behavior as journald's native socket input path.
   - Recommendation: A if exact journald app emulation is the priority; add
     optional diagnostics if available without complicating hot paths.
   - Decision: A. JOURNAL-APP drops invalid/protected caller fields and writes
     the remaining valid fields. If no valid caller fields remain, the append
     fails as an empty entry.

3. Producer remapping placement.
   - Earlier working note: keep remapping as an explicit producer
     adapter/helper above the base policy levels.
   - User correction: no producer-specific remapping belongs in this SDK.
   - Decision: remove remapping behavior from SDK code, docs, and public API.

## Implications And Decisions

User decisions already recorded:

1. The SDK must distinguish RAW, JOURNALD, and JOURNAL-APP field-name policy
   levels.
2. RAW allows what the journal file structures allow and does not guarantee
   systemd ecosystem compatibility.
3. RAW still requires the first `=` split and therefore field names cannot
   contain `=`.
4. JOURNALD and JOURNAL-APP must generate systemd-friendly files for stock
   tooling.
5. JOURNALD must provide the freedom journald has, including trusted fields.
6. JOURNAL-APP must enforce the restrictions journald applies to applications.
7. Any consumer may impose extra rules before calling the SDK, but the SDK must
   not ship producer-specific remapping behavior.

Implementation-blocking decisions still needed:

- None. The current implementation target is:
  - default high-level policy: JOURNALD;
  - JOURNAL-APP invalid/protected caller fields: drop, then fail only if no
    valid caller fields remain;
  - remapping: removed from SDK scope.

## Plan

1. Spec and skill correction.
   - Scope: `.agents/sow/specs/product-scope.md` and
     `.agents/skills/project-journal-compatibility/SKILL.md`.
   - Risk: low implementation risk, high contract importance.
2. API design and shared fixtures.
   - Scope: define common policy names, behavior tables, and cross-language
     fixtures.
   - Risk: public API consistency across languages.
3. Low-level writer policy split.
   - Scope: Rust, Go, Node.js, Python direct-file writers.
   - Risk: RAW mode may expose reader/indexer assumptions.
4. High-level directory writer policy split.
   - Scope: Rust, Go, Node.js, Python `Log` / directory writers.
   - Risk: default behavior changes from `v0.2.0`.
5. Tests and interoperability.
   - Scope: unit, conformance, stock-reader, and SDK-reader tests.
   - Risk: RAW mode cannot be validated by stock systemd for incompatible names.
6. Docs and release prep.
   - Scope: READMEs, `go/API.md`, examples, and release-tag process if a patch
     release is requested.
   - Risk: Go module tag must use `go/vX.Y.Z`.

## Delegation Plan

Implementer:

- Current routing is local implementation by the project manager; no external
  implementer agents unless the user explicitly changes this.

Reviewers:

- Use read-only reviewers after a meaningful implementation batch:
  - `llm-netdata-cloud/glm-5.1`
  - `llm-netdata-cloud/kimi-k2.6`
  - `llm-netdata-cloud/qwen3.6-plus`
  - `llm-netdata-cloud/minimax-m2.7-coder`
- Skip `llm-netdata-cloud/mimo-v2.5-pro` while out of quota.

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

- Record failed validations, reviewer findings, and model failures in this SOW.
- Do not close the SOW until the audit passes, specs/docs are aligned, and
  reviewer findings are resolved or explicitly dispositioned.

## Execution Log

### 2026-05-28

- Created this pending SOW after the user clarified the RAW / JOURNALD /
  JOURNAL-APP model and corrected the RAW first-`=` structural rule.
- Performed read-only analysis of current SDK code, Netdata vendored Rust, and
  systemd v260.1 source.
- Activated the SOW for implementation and release after the user requested all
  languages/modes, tests, and a `v0.3.0` release for SNMP traps integration.
- Removed producer-specific remapping code and public API from Rust, Go,
  Node.js, and Python.
- Added the shared `RAW`, `JOURNALD`, and `JOURNAL-APP` field-name policies to
  direct-file writers and high-level directory writers in all four languages.
- Removed Rust reader, indexer, query, facet, cache, and provenance behavior
  that depended on project-specific field-name mapping metadata.
- Updated public README/API docs, product scope specs, and the journal
  compatibility project skill to make producer-specific transformations
  consumer-owned behavior outside the SDK.
- Updated SOW-0009 benchmark planning language so future performance work
  measures field-name policy validation/filtering instead of removed remapping
  behavior.
- Validation and reviewer cycles found and fixed:
  - missing Rust low-level payload assertions for `JOURNAL-APP` and `RAW`;
  - missing high-level `Log` `RAW` coverage outside Go;
  - missing Rust high-level structured `write_fields` policy coverage;
  - undocumented Node.js/Python policy aliases;
  - dead legacy Go/Python validation helpers;
  - stale docs/comments that implied producer-specific behavior.

## Validation

Acceptance criteria evidence:

- Specs define all three field-name policy layers and state that
  producer-specific field-name transformations are outside SDK behavior:
  `.agents/sow/specs/product-scope.md`.
- Runtime journal compatibility skill records the same cross-language policy
  contract: `.agents/skills/project-journal-compatibility/SKILL.md`.
- Rust exposes `FieldNamePolicy` through `journal-core`,
  `journal-log-writer`, and `journal`; Go exposes `FieldNamePolicy`; Node.js
  and Python expose `FIELD_NAME_POLICY_JOURNALD`, `FIELD_NAME_POLICY_RAW`, and
  `FIELD_NAME_POLICY_JOURNAL_APP`.
- Direct-file and high-level directory writer tests cover `JOURNALD`,
  `JOURNAL-APP`, and `RAW` behavior across Rust, Go, Node.js, and Python.
- Stock `journalctl --verify` / JSON readback remains covered for
  `JOURNALD` and `JOURNAL-APP` generated files. RAW mode is validated through
  SDK readers/snapshots and is not claimed as stock systemd-compatible when
  field names violate systemd rules.
- Scans found no remaining `ND_REMAPPING`, `ND_*` marker/prefix behavior,
  `field_remap`, `field-remap`, `field_map`, `REMAPPING_MARKER`, or `rdp`
  implementation references in SDK code or public docs. Remaining `remap`
  matches are mmap/window remapping or historical SOW analysis.

Tests or equivalent validation:

- `go test ./...` from `go/`: passed.
- `npm test` from `node/`: passed.
- `PYTHONPATH=.local/python-deps:python python python/test_all.py`: passed.
- `cargo test -q --manifest-path rust/Cargo.toml --workspace`: passed.
- `node --check node/src/lib/writer.js && node --check node/test/all.js`:
  passed during implementation.
- `python -m py_compile python/journal/writer.py python/test_all.py`: passed
  during implementation.

Real-use evidence:

- The release target remains SNMP traps integration via Go module tags. Real
  Netdata SNMP traps throughput/integration validation will happen outside this
  SOW after `v0.3.0` / `go/v0.3.0` is published.

Reviewer findings:

- Minimax, Qwen, GLM, and Kimi reviewer runs all returned
  `PRODUCTION GRADE`.
- Reviewer findings fixed before closure:
  - Rust low-level policy tests lacked payload assertions: fixed in
    `rust/src/crates/journal-core/src/file/writer.rs`.
  - Go high-level `Log` lacked RAW coverage: fixed in
    `go/journal/log_test.go`.
  - Node.js/Python default JOURNALD invalid-name rejection coverage was thin:
    fixed in `node/test/all.js` and `python/test_all.py`.
  - Node.js/Python accepted undocumented policy aliases: removed and tested.
  - Rust/Node.js/Python high-level `Log` RAW coverage was missing: fixed in
    `rust/src/crates/journal-log-writer/tests/log_writer.rs`,
    `node/test/all.js`, and `python/test_all.py`.
  - Rust high-level structured `write_fields` policy coverage was missing:
    fixed in `rust/src/crates/journal-log-writer/tests/log_writer.rs`.
  - Dead legacy Go/Python default validation helpers: removed.
  - SOW validation section was stale: updated here before closure.
- Low non-blocking finding accepted:
  - Rust low-level direct `JournalApp` returns `JournalError::InvalidField`
    when every field is filtered. High-level `Log` returns
    `WriterError::EmptyEntry`, matching the public high-level contract. The
    low-level behavior is retained as a direct writer error classification.
  - Node.js and Python reader field-map APIs are string-keyed, so RAW
    non-UTF8 field names are preserved through payload snapshots but not as
    exact map keys. This is pre-existing reader API behavior, not a writer
    policy regression. It is tracked separately in
    `.agents/sow/pending/SOW-0039-20260528-raw-byte-field-name-reader-representation.md`.

Same-failure scan:

- Removed-file and pattern scans covered Rust, Go, Node.js, Python, specs,
  skills, docs, provenance, and current SOW files. The removed pattern no
  longer exists in SDK code or public docs.
- `.agents/sow/audit.sh`: passed before lifecycle close while SOW-0038 was
  still in `.agents/sow/current/`.
- `.agents/sow/audit.sh`: passed again after moving SOW-0038 to
  `.agents/sow/done/` and adding SOW-0039 to `.agents/sow/pending/`.

Sensitive data gate:

- This SOW contains no raw secrets, credentials, bearer tokens, SNMP community
  strings, customer names, personal data, customer-identifying IP addresses,
  private endpoints, or proprietary incident details.
- Examples use only synthetic field names and values.

Artifact maintenance gate:

- AGENTS.md: not changed; no workflow or project-wide guardrail change was
  needed.
- Runtime project skills: `.agents/skills/project-journal-compatibility/SKILL.md`
  updated with the field-name policy layer contract and no-remapping rule.
- Specs: `.agents/sow/specs/product-scope.md` updated with the current product
  contract.
- End-user/operator docs: Rust, Go, Node.js, and Python README/API docs updated
  to document the three policies and remove producer-specific remapping
  behavior.
- End-user/operator skills: no current output/reference skills affected.
- SOW lifecycle: SOW-0038 is completed and moved to `.agents/sow/done/`;
  SOW-0037 remains paused for this release-critical correction.
- SOW-status.md: updated to list SOW-0038 as recently completed and SOW-0039
  as pending follow-up.

Specs update:

- Completed in `.agents/sow/specs/product-scope.md`.

Project skills update:

- Completed in `.agents/skills/project-journal-compatibility/SKILL.md`.

End-user/operator docs update:

- Completed in `rust/README.md`, `go/README.md`, `go/API.md`,
  `node/README.md`, and `python/README.md`.

End-user/operator skills update:

- No output/reference skills exist for this repository, so no update was
  needed.

Lessons:

- Do not treat a producer-specific remapping convenience as a base journal SDK
  field-name policy.
- Keep public SDK field-name behavior producer-neutral. Producer-specific
  naming schemes belong in consumers before they call the SDK.

Follow-up mapping:

- No deferred writer field-name policy behavior remains.
- RAW non-UTF8 field-name reader representation is tracked separately in
  `.agents/sow/pending/SOW-0039-20260528-raw-byte-field-name-reader-representation.md`.

## Outcome

Completed. The SDK now exposes RAW, JOURNALD, and JOURNAL-APP field-name
policy layers across Rust, Go, Node.js, and Python; removes producer-specific
field-name remapping from SDK writers, readers, indexers, query paths, docs,
and public API; and is ready for `v0.3.0` / `go/v0.3.0` release tags on the
closure commit.

## Lessons Extracted

- Field-name policy must be modeled as a base journal compatibility contract,
  not as a producer-specific transformation layer.
- Tests must cover direct-file and high-level directory writer APIs separately
  because both are public surfaces.
- Current/planned SOWs must be updated when a product contract changes, or
  future work can accidentally reintroduce removed behavior.

## Followup

- `.agents/sow/pending/SOW-0039-20260528-raw-byte-field-name-reader-representation.md`
  tracks the reader API representation gap for RAW non-UTF8 field names in
  string-keyed Node.js and Python field maps.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and
later testing or use found broken behavior. Use a dated
`## Regression - YYYY-MM-DD` heading at the end of the file. Never prepend
regression content above the original SOW narrative.
