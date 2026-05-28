# SOW-0041 - Node.js Writer Rust Parity

## Status

Status: completed

Sub-state: completed on 2026-05-28 after validation, reviewer confirmation, and
SOW lifecycle closure.

## Requirements

### Purpose

Align Node.js writer API and file-format behavior with the finalized Rust writer
contract, while recording runtime-specific implementation limits honestly.

### User Request

The user agreed Node.js must align its API and internal operations with Rust
after Rust and Go writer closure. Node.js may not support the same mmap strategy
in the common runtime path, but this must be evidence-based.

### Assistant Understanding

Facts:

- Node.js writer currently uses Buffer construction plus positioned
  `fs.readSync()` / `fs.writeSync()` calls rather than a Rust-like mmap
  strategy.
- Node.js writer must share the same field-name policy modes and public writer
  API concepts as the other languages.
- SOW-0037 initially suspected a Node.js cooperative writer lock contention bug
  from a too-short lock-matrix run. A longer all-language lock run passed, so
  there is no known Node.js lock bug at SOW activation time.
- Common compression libraries are allowed, including packages that provide a
  maintainable pure-runtime or acceptable non-linking path.
- Node.js has no built-in mmap API in the documented `node:fs` module. The
  documented synchronous positioned file operations available to this SDK are
  `fs.readSync()`, `fs.readvSync()`, `fs.writeSync()`, and `fs.writevSync()`.
  Source: Node.js v26.2.0 file-system docs at `https://nodejs.org/api/fs.html`.

Inferences:

- The target is API and compatibility parity first; performance parity may be
  limited by runtime constraints.
- Adding a Node.js mmap package would be a separate dependency/runtime-policy
  decision because common mmap packages require native addons. This SOW will
  not add such a dependency unless evidence shows an acceptable no-native
  runtime path.

Unknowns:

- Whether a future no-native Node.js mmap path exists and measurably improves
  performance enough to justify a dependency.

### Acceptance Criteria

- Node.js writer API and options match the agreed writer contract from SOW-0037.
- Node.js writer supports the same field-name policy modes and raw/structured
  append semantics.
- Node.js writer continues to participate in the same cooperative lock contract
  as Rust and Go, including contention rejection and stale lock cleanup.
- Node.js writer internal behavior is aligned with Rust where practical, and
  every runtime-specific difference is recorded with evidence.
- Node.js writer passes shared writer conformance and interoperability tests.
- Node.js writer outputs remain readable by stock systemd tooling where the
  selected policy mode is systemd-friendly.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `node/src/lib/writer.js`
- `node/src/lib/directory-writer.js`
- `node/test/all.js`
- `node/README.md`
- `go/journal/writer.go`
- `go/journal/log.go`
- `python/journal/writer.py`
- `python/journal/directory_writer.py`
- `.agents/sow/specs/product-scope.md`
- `.agents/sow/done/SOW-0037-20260527-reference-drift-audit.md`
- `.agents/sow/done/SOW-0040-20260528-python-writer-mmap-and-rust-parity.md`
- `.agents/sow/specs/product-scope.md`

Current state:

- Node.js writer is functionally capable but has runtime-specific file access
  and allocation behavior that must be classified. Its cooperative writer lock
  implementation passed the longer all-language lock matrix, so lock
  validation remains a required regression check rather than a known bug fix.
- Direct Node.js `Writer.append()` is structured-only. It builds full
  `KEY=value` payloads internally in `node/src/lib/writer.js:266`, but there is
  no public `appendRaw()` method next to `appendMap()`.
- Node.js field-name policy helpers exist in `node/src/lib/writer.js:1094`,
  `node/src/lib/writer.js:1100`, and `node/src/lib/writer.js:1119`, so raw
  append should reuse the existing policy model rather than introduce a second
  validation path.
- Node.js high-level `Log.append()` exists in
  `node/src/lib/directory-writer.js:136`, but there is no `Log.appendRaw()`.
- Node.js high-level `_fieldsForAppend()` currently appends only
  `_SOURCE_REALTIME_TIMESTAMP` when supplied and does not inject indexed
  `_BOOT_ID=<boot-id>` DATA metadata in
  `node/src/lib/directory-writer.js:473`.
- Go and Python now expose the target parity shape:
  `go/journal/writer.go:317` and `python/journal/writer.py:386` expose raw
  direct-file append; `go/journal/log.go:454` and
  `python/journal/directory_writer.py:274` expose high-level raw append;
  `go/journal/log.go:1118` and `python/journal/directory_writer.py:505`
  inject high-level `_BOOT_ID` plus optional `_SOURCE_REALTIME_TIMESTAMP`.

Risks:

- Native addon dependencies could violate the runtime policy if they are loaded
  in the SDK path.
- Trying to force Rust internals into Node.js may reduce maintainability without
  helping users.

## Pre-Implementation Gate

Status: ready for implementation

Problem / root-cause model:

- SOW-0037 and SOW-0040 closed the writer contract for Rust/Go/Python. Node.js
  now lags that contract in two API/metadata surfaces: raw full-payload append
  and high-level `_BOOT_ID` DATA injection.
- Node.js cannot be made mmap-equivalent to Rust/Python with only built-in
  Node.js APIs. For this SOW, the correct target is contract and compatibility
  parity with an explicit runtime-specific file-access limitation.

Evidence reviewed:

- Node.js direct writer structured-only API:
  `node/src/lib/writer.js:266-355`.
- Node.js field policy helpers: `node/src/lib/writer.js:1094-1157`.
- Node.js high-level writer append and metadata helper:
  `node/src/lib/directory-writer.js:136-153` and
  `node/src/lib/directory-writer.js:473-481`.
- Go target raw/direct and high-level metadata shape:
  `go/journal/writer.go:317-332`, `go/journal/log.go:454-486`, and
  `go/journal/log.go:1118-1147`.
- Python target raw/direct and high-level metadata shape:
  `python/journal/writer.py:386-392`,
  `python/journal/directory_writer.py:274-286`, and
  `python/journal/directory_writer.py:505-527`.
- Product writer contract: `.agents/sow/specs/product-scope.md:154-182`.
- Node.js official file-system documentation, v26.2.0:
  `https://nodejs.org/api/fs.html`.

Affected contracts and surfaces:

- Node.js direct `Writer` public API.
- Node.js high-level `Log` public API.
- Node.js directory writer metadata semantics for `_BOOT_ID` and
  `_SOURCE_REALTIME_TIMESTAMP`.
- Node.js field-name policy behavior for structured and raw payload paths.
- Node.js README/API documentation and product-scope spec.
- Shared interoperability, live, lock, compression, compact, and stock verify
  matrices for the Node.js writer slice.

Existing patterns to reuse:

- Rust and Go writer contracts from SOW-0037.
- Python parity implementation and tests from SOW-0040.
- Existing Node.js tests and shared conformance fixtures.

Risk and blast radius:

- Medium for Node.js writer users because a new public raw API and additional
  indexed metadata become visible.
- Low for Rust/Go/Python because this SOW should not touch those
  implementations except for shared specs/docs if needed.
- Compatibility risk: high-level `Log` entries will gain `_BOOT_ID` as an
  indexed DATA field, matching Rust/Go/Python and enabling file-backed
  `--boot` filtering; any Node consumer that counted exact field sets may see
  one additional system metadata field.
- Dependency risk: no new dependency is planned. Adding native mmap would
  violate current runtime constraints unless separately approved.

Sensitive data handling plan:

- Use synthetic fixtures only. Do not record real logs, SNMP communities,
  customer data, personal data, credentials, bearer tokens, private endpoints,
  or production incident details.

Implementation plan:

1. Factor Node.js direct append through a shared payload append path.
2. Add `Writer.appendRaw(payloads, options)` using the existing policy
   validators and the same DATA/FIELD/ENTRY object path as structured append.
3. Add `Log.appendRaw(payloads, options)` with high-level policy filtering,
   rotation, retention, active-writer lifecycle, and metadata injection.
4. Change structured `Log.append()` to prepend indexed `_BOOT_ID=<boot-id>` and
   optional `_SOURCE_REALTIME_TIMESTAMP=<usec>`, matching Rust/Go/Python.
5. Add tests for raw full-payload binary values, structured/raw byte identity,
   high-level `_BOOT_ID` visibility, JOURNAL-APP raw filtering, and stock
   query behavior.
6. Update Node.js docs and product-scope spec for the public API and
   runtime-specific no-mmap limitation.
7. Align API and internal behavior where practical, including regression
   coverage for the cooperative writer lock contract.
8. Record measured runtime-specific differences.

Validation plan:

- `npm test`.
- Focused Node.js raw append and directory metadata tests.
- `tests/interoperability/run_binary_matrix.py --writers node --readers stock go rust node python`.
- `tests/interoperability/run_compression_matrix.py --writers node --readers stock go rust node python --compression zstd xz lz4 --entries 2`.
- `tests/interoperability/run_compact_matrix.py --writers node --readers stock go rust node python --entries 2 --compression none`.
- `tests/interoperability/run_lock_matrix.py --entries 200 --delay-ms 20`.
- `tests/interoperability/run_live_matrix.py --writers node --readers stock go rust node python --features regular compact zstd xz lz4 compact-zstd compact-xz compact-lz4 sealed --entries 20 --writer-delay-ms 20`.
- `git diff --check`.
- `.agents/sow/audit.sh`.
- Read-only reviewer pass from the approved reviewer pool.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: update only if dependency/runtime workflow changes.
- Specs: update Node.js writer feature slice.
- End-user/operator docs: update Node.js API docs.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: move to current while in progress; move to done only after
  validation and reviewer acceptance.
- SOW-status.md: updated when activated and closed.

Open-source reference evidence:

- Node.js file-system API documentation was checked at
  `https://nodejs.org/api/fs.html` on 2026-05-28.
- No external source repository was checked for this SOW activation.

Open decisions:

- None. No new Node.js mmap dependency will be added in this SOW.

## Implications And Decisions

- 2026-05-28: user agreed Node.js writer parity follows Rust/Go writer closure.
- 2026-05-28: SOW-0037 follow-up validation corrected the earlier Node.js lock
  bug suspicion. The short-hold matrix failure was a timing artifact; a longer
  all-language lock matrix passed.
- 2026-05-28: Node.js runtime-specific file access remains Buffer plus
  positioned `node:fs` reads/writes for this SOW. No native mmap dependency is
  introduced.

## Plan

1. Implement Node.js writer raw append and shared payload path.
2. Implement Node.js high-level raw append and metadata injection.
3. Add/adjust tests and docs.
4. Run validation matrices.
5. Run read-only reviewer pass and resolve findings.

## Delegation Plan

Implementer:

- Local implementation by the project manager unless the user changes routing.

Reviewers:

- Read-only reviewers from the approved pool.

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

- Record runtime limits, dependency findings, reviewer findings, and benchmark
  failures in this SOW before changing scope.

## Execution Log

### 2026-05-28

- Created from the agreed SOW restructuring.
- Activated after SOW-0037 and SOW-0040 completed.
- Implemented Node.js direct-file `Writer.appendRaw()` and a shared
  `_appendPayloads()` path for structured and raw append.
- Implemented Node.js high-level `Log.appendRaw()`.
- Updated high-level `Log.append()` and `Log.appendRaw()` to prepend indexed
  `_BOOT_ID=<boot-id>` and optional `_SOURCE_REALTIME_TIMESTAMP=<usec>`.
- Added raw-payload validation tests for empty input, empty raw field names,
  missing separators, binary values, JOURNAL-APP filtering, direct low-level
  no-metadata behavior, and structured/raw byte identity.
- Added high-level raw and structured tests for metadata injection, default
  JOURNALD protected-field behavior, JOURNAL-APP filtering, and duplicate
  same-value `_BOOT_ID` deduplication.
- Updated Node.js README and product-scope spec.
- First reviewer pass:
  - `glm`: PRODUCTION GRADE; requested SOW validation updates and additional
    test coverage for raw/metadata edge cases.
  - `qwen`: PRODUCTION GRADE; requested explicit empty-field-name,
    low-level-no-metadata, JOURNALD raw, and duplicate `_BOOT_ID` coverage.
  - `minimax`: PRODUCTION GRADE; requested SOW validation updates and noted
    minor coverage gaps.
  - `kimi`: PRODUCTION GRADE; requested symmetry/test-gap cleanup.
- Addressed first-pass findings by adding the raw and high-level metadata
  tests listed above.
- Second reviewer pass:
  - `glm`: PRODUCTION GRADE; requested only SOW validation completion and noted
    a non-blocking structured JOURNALD `_BOOT_ID` assertion gap.
  - `minimax`: NOT PRODUCTION GRADE only because this SOW validation section
    was still pending; also requested a structured `_BOOT_ID` duplicate test
    and a comment explaining `JOURNAL-APP` Log-to-Writer policy mapping.
  - `kimi` and `qwen` second-pass jobs stalled after reading files and were
    stopped with exact PIDs `514278`, `514292`, `514290`, and `514306`. Their
    first pass was PRODUCTION GRADE, and the actionable findings they had
    already raised were addressed.
- Addressed second-pass findings by adding the structured JOURNALD `_BOOT_ID`
  duplicate assertion, documenting `writerPolicyForLogPolicy()`, and filling
  this validation gate.
- Final confirmation review:
  - `minimax`: PRODUCTION GRADE; no blocking findings.
  - `glm`: PRODUCTION GRADE; no blocking findings. It independently reran
    `git diff --check` and `npm test`, both passing.

## Validation

Acceptance criteria evidence:

- Node.js direct writer now exposes `appendRaw(payloads, opts)` in
  `node/src/lib/writer.js`.
- Node.js high-level directory writer now exposes `appendRaw(payloads, opts)`
  in `node/src/lib/directory-writer.js`.
- High-level `Log` structured and raw append paths prepend indexed
  `_BOOT_ID=<boot-id>` and optional `_SOURCE_REALTIME_TIMESTAMP=<usec>`.
- Node.js field-name policy behavior remains shared through
  `prepareFieldsForPolicy()` and `prepareRawPayloadsForPolicy()`.
- No mmap or native dependency was added; Node.js file access remains Buffer
  plus positioned `node:fs` reads/writes.
- Product spec and Node.js README document the public API and runtime limit.

Tests or equivalent validation:

- `npm_config_cache=$PWD/.local/npm-cache npm --prefix node test` passed after
  implementation and passed again after reviewer-driven test additions.
- `tests/interoperability/run_binary_matrix.py --writers node --readers stock go rust node python`
  passed 13/13; result:
  `.local/interoperability/binary-matrix-results-20260528-211734.json`.
- `tests/interoperability/run_compression_matrix.py --writers node --readers stock go rust node python --compression zstd xz lz4 --entries 2`
  passed 54/54; result:
  `.local/interoperability/compression-matrix-results-20260528-211745.json`.
- `tests/interoperability/run_compact_matrix.py --writers node --readers stock go rust node python --entries 2 --compression none`
  passed 14/14; result:
  `.local/interoperability/compact-matrix-none-results-20260528-211752.json`.
- `tests/interoperability/run_lock_matrix.py --entries 200 --delay-ms 20`
  passed 8/8; result:
  `.local/interoperability/lock-matrix-results-20260528-211813.json`.
- `tests/interoperability/run_live_matrix.py --writers node --readers stock go rust node python --features regular compact zstd xz lz4 compact-zstd compact-xz compact-lz4 sealed --entries 20 --writer-delay-ms 20`
  passed 9/9; result:
  `.local/interoperability/live-feature-matrix-results-20260528-211828.json`.
- Stock systemd observed by the matrices: `systemd 260 (260.1-2-manjaro)`.
- `git diff --check` passed after implementation and after reviewer-driven
  test additions.
- `.agents/sow/audit.sh` passed after moving SOW-0041 to `done/` and updating
  `SOW-status.md`.

Real-use evidence:

- Stock `journalctl --verify --file`, stock JSON/export reads, stock
  libsystemd reads, and Go/Rust/Node.js/Python repository readers all accepted
  Node.js writer output in the binary, compression, compact, lock, and live
  matrices listed above.
- Live matrix evidence covered stock readers and repository readers while the
  Node.js writer was actively appending regular, compact, compressed,
  compact-compressed, and sealed files.

Reviewer findings:

- First-pass `glm`: PRODUCTION GRADE. Findings were test/SOW hygiene only.
  Disposition: fixed by adding direct raw no-metadata coverage, raw empty-name
  coverage, high-level raw JOURNALD coverage, duplicate `_BOOT_ID` coverage,
  and this validation section.
- First-pass `qwen`: PRODUCTION GRADE. Findings were low-risk test gaps.
  Disposition: fixed by the same test additions.
- First-pass `minimax`: PRODUCTION GRADE. Finding was SOW validation pending.
  Disposition: fixed in this section.
- First-pass `kimi`: PRODUCTION GRADE. Finding was minor empty-check/test
  symmetry. Disposition: fixed through explicit empty input and raw validation
  tests.
- Second-pass `glm`: PRODUCTION GRADE. Findings were SOW validation pending
  and a structured JOURNALD `_BOOT_ID` assertion gap. Disposition: fixed.
- Second-pass `minimax`: NOT PRODUCTION GRADE only because the SOW validation
  gate was still empty; also requested a structured duplicate `_BOOT_ID` test
  and policy-mapping comment. Disposition: fixed.
- Second-pass `kimi` and `qwen`: stopped after stalling; no final second-pass
  verdict was produced. Their first-pass verdicts were PRODUCTION GRADE and
  their already-reported findings were fixed.
- Final confirmation `minimax` and `glm`: PRODUCTION GRADE with no blocking
  findings after SOW validation was filled and the structured `_BOOT_ID`
  assertion/comment were added.

Same-failure scan:

- Searched Node.js test and writer paths for raw append, `_BOOT_ID`,
  `_SOURCE_REALTIME_TIMESTAMP`, and field policy coverage.
- Added both direct writer and high-level Log coverage because the same
  missing raw/metadata patterns could occur at either API layer.
- Confirmed Rust/Go/Python already expose the target raw and metadata patterns
  in SOW-0037 and SOW-0040 evidence; this SOW did not modify those languages.

Sensitive data gate:

- Passed. Tests and docs use synthetic UUIDs, synthetic messages, and local
  temporary files only. No real logs, credentials, SNMP communities, customer
  data, personal data, bearer tokens, private endpoints, or production incident
  details were written to durable artifacts.

Artifact maintenance gate:

- `AGENTS.md`: no update needed; workflow and routing rules did not change.
- Runtime project skills: no update needed; no new compatibility or
  orchestration rule was discovered.
- Specs: `.agents/sow/specs/product-scope.md` updated for Node.js raw append,
  high-level metadata injection, and no-mmap runtime limit.
- End-user/operator docs: `node/README.md` updated for `appendRaw()`,
  `Log.appendRaw()`, metadata injection, and no-mmap runtime limit.
- End-user/operator skills: no output/reference skill exists for this SDK
  slice.
- SOW lifecycle: SOW-0041 was moved from pending to current and
  `SOW-status.md` updated.
- `SOW-status.md`: updated when SOW-0041 activated; will be updated again at
  closure.

Specs update:

- Updated `.agents/sow/specs/product-scope.md` current Node.js writer feature
  slice to include direct and high-level raw append, high-level `_BOOT_ID` /
  `_SOURCE_REALTIME_TIMESTAMP` injection, and Buffer plus positioned `node:fs`
  file access without native mmap dependency.

Project skills update:

- No project skill update was needed. The implementation followed existing
  `project-agent-orchestration` and `project-journal-compatibility` rules.

End-user/operator docs update:

- Updated `node/README.md` with `writer.appendRaw()`, `log.appendRaw()`,
  high-level metadata injection, and the Node.js file-access limitation.

End-user/operator skills update:

- No end-user/operator skill artifact exists for the Node.js SDK writer API.

Lessons:

- Reviewer passes against a pending SOW validation section predictably block on
  process completeness. Future review prompts should either run before close
  with the expected SOW-pending state called out explicitly, or after the
  validation evidence has been drafted.
- For each language parity SOW, direct writer and high-level writer tests must
  cover both structured and raw API layers; reviewers correctly found the gaps
  when only one layer had explicit assertions.

Follow-up mapping:

- No new follow-up SOW is required from this work. Remaining writer
  certification and performance work stays tracked by SOW-0042.

## Outcome

Node.js writer parity is implemented and validated for the SOW-0041 scope.
Direct and high-level raw append APIs now exist, high-level metadata injection
matches Rust/Go/Python, Node.js docs/specs record the runtime-specific no-mmap
limit, and stock/cross-language matrices passed for the affected writer slice.

## Lessons Extracted

- Complete SOW validation evidence before asking reviewers for a final
  production-grade verdict, or make clear in the prompt that the validation
  section will be filled after reviewer dispositions.
- Parity tests should be symmetric across direct writer and high-level writer
  APIs, and across structured and raw append forms.

## Followup

None. SOW-0042 remains the existing follow-up for writer certification and
performance work.

## Regression Log

None yet.
