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
accepted.

### User Request

The user asked to check for important differences first between systemd and
Rust, and then between Go and Rust. The user stated that Rust should remain as
close to systemd as possible, or differences must be conscious rather than
accidental. Rust should then become the reference for Go, and later for Node.js
and Python.

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

Unknowns:

- Which Rust/systemd differences are harmless implementation details and which
  affect correctness, live-reader behavior, performance, retention, verification,
  or future compatibility.
- Which Go/Rust differences are accidental versus legitimate language/runtime
  choices.
- Whether Node.js and Python should be audited in this SOW or represented by a
  follow-up SOW after Rust and Go are classified.

### Acceptance Criteria

- Produce a Rust-versus-systemd difference matrix for journal writer, reader,
  directory, verification, retention, mmap/publication, compression, compact,
  FSS, and journalctl/file-backed behavior.
- Produce a Go-versus-Rust difference matrix for the same surfaces where Go has
  implementation.
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

Evidence reviewed:

- `systemd/systemd @ c0a5a2516d28`
  - `src/libsystemd/sd-journal/mmap-cache.c`: systemd windowed mmap cache.
  - `src/libsystemd/sd-journal/journal-file.c`: journal file allocation,
    object movement, post-change notification, verification.
  - `src/libsystemd/sd-journal/sd-journal.c`: reader mmap cache ownership.
  - `src/journal/journald-manager.c`: writer mmap cache ownership.
- `rust/src/crates/journal-core/src/file/mmap.rs`: Rust window manager.
- `rust/src/crates/journal-core/src/file/file.rs`: Rust journal file
  reader/writer mmap setup and options.
- `go/journal/mmap_unix.go`: Go Unix writer whole allocated-file mmap.
- `go/journal/reader.go`: Go `ReadAt()` reader paths.
- `go/journal/writer.go`: Go writer publication and arena management.
- `.agents/sow/pending/SOW-0036-20260527-live-publication-modes-and-fast-consumers.md`:
  existing mmap/publication measurement candidates.

Affected contracts and surfaces:

- Rust SDK reader/writer behavior and compatibility claims.
- Go SDK reader/writer behavior and compatibility claims.
- Future Node.js and Python reference behavior.
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

Risk and blast radius:

- Medium-to-high analysis risk because the audit spans many journal surfaces.
- Code-change risk is deferred until differences are classified.
- Compatibility risk if a difference is misclassified as implementation detail
  when it affects live readers, corrupt-file handling, FSS, or verification.
- Performance risk if systemd parity is applied mechanically where Rust has a
  conscious faster compatible implementation.

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
5. Present any behavior/API decisions to the user with evidence before code
   changes.
6. Update specs/project skills with the durable reference hierarchy:
   systemd as external file-format authority, Rust as project reference after
   classified differences, other languages aligned to Rust unless explicitly
   accepted.
7. If fixes are small and user decisions are recorded, implement them in this
   SOW; otherwise map each fix to concrete follow-up SOWs.

Validation plan:

- Run same-failure searches for each difference class.
- Run existing matrices relevant to any fixed behavior.
- Run `.agents/sow/audit.sh`.
- Use read-only external reviewers after any implementation change.
- Require every unresolved difference to be classified and mapped before close.

Artifact impact plan:

- AGENTS.md: likely unchanged unless the reference hierarchy needs a
  project-wide guardrail.
- Runtime project skills: likely update
  `.agents/skills/project-journal-compatibility/SKILL.md`.
- Specs: likely update `.agents/sow/specs/product-scope.md`.
- End-user/operator docs: likely unaffected unless public SDK behavior changes.
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

## Implications And Decisions

- Decision 1: Option B. Audit Rust against systemd first, then Go against Rust.
- Decision 2: pending if implementation is started. Recommendation is Option B.

## Plan

1. Audit Rust against systemd and classify differences.
2. Audit Go against Rust and classify differences.
3. Present decisions for differences that affect public behavior, performance,
   or compatibility claims.
4. Fix only approved/local accidental drift or map to follow-up SOWs.
5. Update specs/skills and close after validation.

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
