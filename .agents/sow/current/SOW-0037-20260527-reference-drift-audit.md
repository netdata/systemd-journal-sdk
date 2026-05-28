# SOW-0037 - Writer Reference Closure

## Status

Status: in-progress

Sub-state: activated on 2026-05-28 after the user approved proceeding with the
writer-side closure pass.

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
- `.agents/sow/pending/SOW-0039-20260528-raw-byte-field-name-reader-representation.md`
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

## Validation

Acceptance criteria evidence:

- Go now exposes both writer append shapes required by the writer API hierarchy:
  structured `Append([]Field, EntryOptions)` and raw full-payload
  `AppendRaw([][]byte, EntryOptions)` for both direct-file `Writer` and
  high-level directory `Log`.
- Rust high-level journal-app raw filtering now matches the SOW/spec rule that
  malformed raw payloads are rejected before journal-app field-name filtering.
- Go high-level `Log` now aligns with Rust/systemd journald behavior by writing
  `_BOOT_ID=<boot-id>` as an indexed DATA payload in addition to entry boot-id
  metadata.

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

Real-use evidence:

- Go tests added in this SOW generate journal files and validate them with stock
  `journalctl --file` / `journalctl --directory` through existing helpers.
  Rust log-writer tests also exercise stock `journalctl --file` where the
  package's existing journalctl helpers require it.

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

Same-failure scan:

- `rg` searches checked `_BOOT_ID` handling across Go/Rust tests, Go reader
  synthesis, Rust log writer injection, and systemd journald evidence. The
  remaining low-level `Writer` APIs intentionally do not inject `_BOOT_ID`;
  high-level `Log` now does.

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
- SOW lifecycle: this SOW remains current/in-progress after this implementation
  chunk; it is not ready to close until the full writer closure matrix is done.
- SOW-status.md: updated when the SOW was activated.

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

Pending.

## Lessons Extracted

Pending.

## Followup

- SOW-0040 - Python Writer Mmap And Rust Parity.
- SOW-0041 - Node.js Writer Rust Parity.
- SOW-0042 - Writer Final Certification.
- SOW-0043 - Rust Reader Libsystemd/Jf Parity.

## Regression Log

None yet.
