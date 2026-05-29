# SOW-0061 - Cross-Language Row-Scoped Facade Lifetime

## Status

Status: completed

Sub-state: Go, Node.js, and Python parity with the Rust row-scoped
current-entry facade payload lifetime contract from SOW-0060 is implemented,
validated, reviewed, and ready to close.

## Requirements

### Purpose

Make the libsystemd-like reader facade contract natural for Netdata facets and
other row-oriented consumers in every SDK language: callers may enumerate all
`FIELD=value` payloads for one current row, cache those returned payload
references/objects, receive end-of-row, and then process the cached payloads
before advancing the reader.

### User Request

The user approved option B from the row-lifetime discussion:

- A, copying every field immediately in facets, kills facets performance.
- B, the SDK providing the stronger row-scoped payload lifetime, is the right
  way.
- C is not needed because this SDK's guarantees are intentionally better than
  systemd's documented guarantees.

The user then approved continuing by bringing Go, Node.js, and Python facade
lifetime behavior to the same row-scoped contract as Rust.

### Assistant Understanding

Facts:

- SOW-0060 completed the Rust reference behavior.
- Rust now keeps current-row facade payloads valid until the reader advances to
  another row, seeks, closes, restarts/releases current-entry DATA state, or
  remaps the backing file.
- The product spec already says Go, Node.js, and Python must be brought to the
  same row-scoped facade contract before cross-language reader facade parity is
  claimed.

Inferences:

- The other languages mostly have compatible runtime behavior already because
  uncompressed data is backed by whole-file buffers/mmap and compressed data is
  returned as a fresh decompressed object.
- The work still matters because comments, docs, and tests currently encode or
  imply the older libsystemd-style current-pointer contract, especially in Go.
- The acceptance gate must prove end-of-row does not invalidate cached payloads
  for uncompressed rows, compressed rows, and mixed compressed/uncompressed
  rows where the language supports those file variants.

Unknowns:

- Whether any existing cross-language tests rely on the older "invalid after
  next enumeration call" wording. This should be resolved by local test runs.

### Acceptance Criteria

- Go facade/reader documentation describes the row-scoped current-entry
  lifetime instead of current-pointer-only lifetime.
- Node.js and Python behavior is explicitly covered by tests for the same
  current-row contract.
- Tests cover uncompressed and compressed payloads. Where practical, tests also
  cover mixed compressed/uncompressed rows.
- End-of-data for a row must not be treated as releasing cached payloads.
- Advancing/seeking/clear/restart remain the contract boundary for cached
  payloads.
- No implementation pre-materializes every field of every row merely to satisfy
  the facade lifetime contract.
- Validation runs for Go, Node.js, Python, SOW audit, and relevant reader
  interoperability.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/specs/product-scope.md`
- `.agents/sow/current/SOW-0009-20260523-benchmark-profile-optimize.md`
- `.agents/sow/done/SOW-0060-20260529-rust-reader-absolute-hot-path-profiling.md`
- `go/journal/reader.go`
- `go/journal/facade.go`
- `go/journal/facade_test.go`
- `node/src/lib/reader.js`
- `node/src/facade.js`
- `node/test/all.js`
- `python/journal/reader.py`
- `python/journal/facade.py`
- `python/test_all.py`

Current state:

- Go `Reader.EnumerateEntryPayload()` currently documents the old lifetime:
  "valid only until the next reader method call, refresh, or Close"
  (`go/journal/reader.go:1037`).
- Go `SdJournalEnumerateAvailableData()` repeats the old libsystemd-style
  lifetime wording (`go/journal/facade.go:443`).
- Go end-of-row enumeration currently calls `r.clearEntryDataState()`, which
  resets offsets and enumeration index but does not unmap or overwrite the
  returned mmap slice or decompressed allocation (`go/journal/reader.go:1047`;
  clear logic at `go/journal/reader.go:508`).
- Go `readDataPayload()` returns mmap/read-at slices for uncompressed data and
  fresh decoded slices for zstd/xz/lz4 compressed data
  (`go/journal/reader.go:1130`).
- Node.js `FileReader.enumerateEntryPayload()` clears enumeration state at
  end-of-row (`node/src/lib/reader.js:578`) and returns
  `_readDataPayloadAt()` for each field (`node/src/lib/reader.js:624`).
  `parseDataPayload()` returns a `Buffer` slice for uncompressed data and a
  decompressed `Buffer` for compressed data.
- Node.js facade `enumerateAvailableData()` delegates directly to the reader
  when the reader supports entry DATA enumeration (`node/src/facade.js:280`).
- Python `FileReader.enumerate_entry_payload()` clears enumeration state at
  end-of-row (`python/journal/reader.py:585`) and returns
  `_read_data_payload_at()` for each field (`python/journal/reader.py:659`).
  Uncompressed mmap slicing returns a bytes object; compressed data returns a
  decompressed bytes object.
- Python facade `enumerate_available_data()` currently returns `bytes(item)`
  from reader enumeration (`python/journal/facade.py:299`), so returned facade
  objects have independent Python object lifetime for callers.

Risks:

- Go slices can alias an mmap that may be replaced on live refresh. The
  contract must keep refresh/remap as an invalidation boundary.
- Node.js Buffer slices retain the underlying Buffer, so row-scoped retention
  is safe, but tests should not rely on garbage collection timing.
- Python returns bytes for the facade, so the lifetime is stronger but may
  retain per-field copy cost. This is acceptable as a language-specific
  copy-on-iteration facade shape; this SOW must not introduce pre-row
  materialization of every field.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Rust now provides a row-scoped facade lifetime, but Go, Node.js, and Python
  have not proven or documented the same cross-language contract. The likely
  root cause is not a deep format bug; it is missing tests and stale
  libsystemd-style comments.

Evidence reviewed:

- Rust reference outcome: `.agents/sow/done/SOW-0060-20260529-rust-reader-absolute-hot-path-profiling.md`.
- Product contract: `.agents/sow/specs/product-scope.md`.
- Go reader/facade code and comments listed in Analysis.
- Node.js reader/facade code listed in Analysis.
- Python reader/facade code listed in Analysis.

Affected contracts and surfaces:

- Go `Reader.EnumerateEntryPayload()` and `SdJournalEnumerateAvailableData()`.
- Node.js `FileReader.enumerateEntryPayload()` and
  `SdJournalEnumerateAvailableData()`.
- Python `FileReader.enumerate_entry_payload()` and
  `SdJournalEnumerateAvailableData()`.
- Cross-language libsystemd-like facade behavior documented in
  `.agents/sow/specs/product-scope.md`.
- Reader benchmarks indirectly, because facade data enumeration must avoid
  pre-materializing whole rows.

Existing patterns to reuse:

- Rust SOW-0060 tests for whole-file uncompressed, windowed uncompressed,
  compressed, and mixed compressed/uncompressed rows.
- Existing Go package tests in `go/journal/facade_test.go`.
- Existing Node.js package tests in `node/test/all.js`.
- Existing Python package tests in `python/test_all.py`.

Risk and blast radius:

- Low to medium. The intended code changes should be mostly tests and comments.
  If a real implementation gap appears, the blast radius is the reader facade
  only.
- Performance risk is low as long as we avoid collecting all current-row
  payloads just to implement lifetime.
- Compatibility risk is low because row-scoped lifetime is a stronger caller
  guarantee than the old documented libsystemd-style wording.

Sensitive data handling plan:

- Use generated test journals only.
- Do not read live host journals.
- Do not record real logs, SNMP communities, customer identifiers, personal
  data, credentials, bearer tokens, private endpoints, or proprietary incident
  details.

Implementation plan:

1. Add Go row-scoped facade tests for uncompressed and compressed/mixed rows;
   update stale Go lifetime comments.
2. Add Node.js row-scoped facade tests for uncompressed and compressed/mixed
   rows; adjust comments only if misleading comments are found.
3. Add Python row-scoped facade tests for uncompressed and compressed/mixed
   rows; adjust comments/docs only if misleading comments are found.
4. Update product spec if the final language-specific contract needs wording
   refinement.
5. Validate Go, Node.js, Python, reader interoperability, and SOW audit.

Validation plan:

- `GOCACHE=$PWD/.local/go-cache GOMODCACHE=$PWD/.local/go-modcache GOPATH=$PWD/.local/go-path go test ./...` from `go/`.
- `npm_config_cache=$PWD/../.local/npm-cache npm test` from `node/`.
- `PYTHONPATH=python python3 python/test_all.py`.
- `python3 tests/interoperability/run_directory_matrix.py --readers go node python rust stock` if the targeted unit tests pass.
- `.agents/sow/audit.sh`.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: no update expected unless this exposes a reusable
  facade-lifetime validation rule.
- Specs: update `.agents/sow/specs/product-scope.md` only if needed to reflect
  final cross-language reality.
- End-user/operator docs: update only if public README/API docs contain stale
  lifetime wording.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: new active child SOW under SOW-0009.
- SOW-status.md: update to list this active SOW.

Open-source reference evidence:

- No new external open-source source was checked for this SOW. The behavior is
  a project SDK contract that intentionally exceeds stock libsystemd's
  documented pointer lifetime.

Open decisions:

- Resolved by user: use option B, SDK row-scoped current-entry facade payload
  lifetime, and do not require facets to copy every field immediately.
- Resolved by user: option C is not needed because this SDK guarantee is
  intentionally better than systemd's documented guarantee.

## Implications And Decisions

1. 2026-05-29 row-scoped lifetime parity
   - Decision: Go, Node.js, and Python must match the Rust row-scoped facade
     contract.
   - Implication: returned current-row payloads must remain safe for caller use
     after end-of-row enumeration and before the next row operation.
   - Risk: live refresh/remap remains an invalidation boundary and must stay
     documented.

2. 2026-05-29 no facet-copy workaround
   - Decision: do not move the burden to facets or other consumers by requiring
     immediate per-field copies.
   - Implication: SDK facade implementations must provide the stronger
     contract without pre-materializing every field in the current row.

## Plan

1. Implement and validate Go facade lifetime tests and documentation.
2. Implement and validate Node.js facade lifetime tests.
3. Implement and validate Python facade lifetime tests.
4. Run cross-language reader validation and audit.
5. Review the whole SOW once local validation is clean.

## Delegation Plan

Implementer:

- Local implementation by the project manager. No external implementer agents.

Reviewers:

- Read-only reviewers from the approved pool only after the complete SOW is
  locally implemented and validated.

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

- If any language has a real implementation gap, fix that language first and
  rerun its local tests before moving on.
- If a language cannot represent borrowed row-scoped payloads safely, document
  the idiomatic copy-on-iteration behavior and prove it with tests.
- If validation exposes unrelated failures, record them and only fix them here
  if they block this contract.

## Execution Log

### 2026-05-29

- Created this active SOW from the user's approval to continue with Go, Node.js,
  and Python row-scoped facade lifetime parity.
- Ran the initial SOW audit after creation:

  ```text
  .agents/sow/audit.sh
  ```

  Result: PASS.

- Go implementation:
  - Updated `Reader.EnumerateEntryPayload()` and
    `SdJournalEnumerateAvailableData()` comments to document the row-scoped
    current-entry lifetime.
  - Added `TestSdJournalDataPayloadsRemainValidForCurrentRow`.
  - Added `TestSdJournalCompressedMixedDataPayloadsRemainValidForCurrentRow`.
  - Both tests store returned slices without copying, enumerate through
    end-of-row, then validate the cached slices.

- Go validation:

  ```text
  cd go
  gofmt -w journal/reader.go journal/facade.go journal/facade_test.go
  GOCACHE=$PWD/../.local/go-cache \
  GOMODCACHE=$PWD/../.local/go-modcache \
  GOPATH=$PWD/../.local/go-path \
  go test ./...
  ```

  Result: PASS.

- Node.js implementation:
  - Added uncompressed and compressed/mixed facade lifetime tests in
    `node/test/all.js`.
  - Tests store returned `Buffer` objects without copying, enumerate through
    end-of-row, then validate the cached buffers.

- Node.js validation:

  ```text
  cd node
  npm_config_cache=$PWD/../.local/npm-cache npm test
  ```

  Result: PASS.

- Python implementation:
  - Added uncompressed and compressed/mixed facade lifetime tests in
    `python/test_all.py`.
  - Added the new tests to `main()` so they are actually executed.
  - Tests store returned facade `bytes` objects, enumerate through end-of-row,
    then validate the cached objects.

- Python validation:

  ```text
  python3 -m pip install --target .local/python-deps -r python/requirements.txt
  PYTHONPATH=.local/python-deps:python python3 python/test_all.py
  ```

  Result: PASS. The dependency install stayed under `.local/python-deps`.

- Cross-language directory validation:

  ```text
  PYTHONPATH=.local/python-deps:python \
  GOCACHE=$PWD/.local/go-cache \
  GOMODCACHE=$PWD/.local/go-modcache \
  GOPATH=$PWD/.local/go-path \
  npm_config_cache=$PWD/.local/npm-cache \
  python3 tests/interoperability/run_directory_matrix.py \
    --readers go node python rust stock
  ```

  Result: PASS against stock `systemd 260 (260.1-2-manjaro)`.

- Final lifecycle validation:

  ```text
  git diff --check
  .agents/sow/audit.sh
  ```

  Result: PASS after moving this SOW to `.agents/sow/done/`.

- Updated public docs and specs:
  - `go/API.md` now documents row-scoped facade lifetime and callback-scoped
    visitor lifetime separately.
  - `node/README.md` documents the row-scoped `Buffer` lifetime.
  - `python/README.md` documents the row-scoped facade lifetime and Python
    `bytes` return shape.
  - `.agents/sow/specs/product-scope.md` now records Go, Node.js, and Python
    as satisfying the strengthened contract.

## Validation

Acceptance criteria evidence:

- Go documentation and tests prove row-scoped current-entry lifetime for
  uncompressed and compressed/mixed rows.
- Node.js tests prove row-scoped current-entry lifetime for uncompressed and
  compressed/mixed rows.
- Python tests prove row-scoped current-entry lifetime for uncompressed and
  compressed/mixed rows.
- Product spec updated to state current cross-language reality.

Tests or equivalent validation:

- `go test ./...`: PASS.
- `npm test`: PASS.
- `PYTHONPATH=.local/python-deps:python python3 python/test_all.py`: PASS.
- `run_directory_matrix.py --readers go node python rust stock`: PASS against
  stock `systemd 260 (260.1-2-manjaro)`.

Real-use evidence:

- Generated uncompressed and compressed/mixed journal files were read through
  each language facade. No live host journal was used.

Reviewer findings:

- Three read-only reviewers checked the complete SOW and changed surface after
  local validation. All three returned production-grade verdicts with no
  blocking findings.
- Non-blocking observations were dispositioned as follows:
  - No post-advance invalidation negative test: rejected as a requirement for
    this SOW because the contract promises validity until the boundary, not
    forced invalidity after the boundary. The invalidation boundary remains
    documented.
  - Python per-field copy-on-iteration cost: accepted as the Python facade
    shape and explicitly documented; this SOW did not pre-materialize whole
    rows.
  - Compressed tests do not assert compression flag in the new unit tests:
    rejected as a blocker because compression flag behavior is already covered
    by the existing compression and directory matrices. These tests target
    payload lifetime after row enumeration.

Same-failure scan:

- Stale lifetime wording scan found old Go wording in `go/API.md` and Go
  source comments. Those were updated. Node.js and Python README files now
  include explicit row-scoped lifetime wording.
- The same facade lifetime test shape was added across Go, Node.js, and
  Python for uncompressed and compressed/mixed rows.

Sensitive data gate:

- Passed. Tests use generated fixtures only and no real credentials, bearer
  tokens, SNMP communities, customer identifiers, personal data, private
  endpoints, or proprietary incident details.

Artifact maintenance gate:

- AGENTS.md: no update expected; existing workflow rules already cover this
  SOW.
- Runtime project skills: no update expected; this did not add a durable new
  workflow.
- Specs: updated `.agents/sow/specs/product-scope.md`.
- End-user/operator docs: updated `go/API.md`, `node/README.md`, and
  `python/README.md`.
- End-user/operator skills: no output/reference skill affected.
- SOW lifecycle: active child SOW created under SOW-0009 and closed as a
  completed child SOW.
- SOW-status.md: updated to move SOW-0061 to Done.

Specs update:

- Updated `.agents/sow/specs/product-scope.md`.

Project skills update:

- No project skill update needed. The existing compatibility skill already
  requires facade parity and shared validation.

End-user/operator docs update:

- Updated `go/API.md`, `node/README.md`, and `python/README.md`.

End-user/operator skills update:

- No output/reference skill affected.

Lessons:

- Python `test_all.py` uses explicit function calls in `main()`. Adding a test
  function is not enough; new tests must be added to `main()`.
- Cross-language facade lifetime tests should use the same shape: cache
  payload references/objects without caller copies, enumerate to end-of-row,
  then validate cached payloads.

Follow-up mapping:

- No follow-up SOW is required from this SOW. The broader reader performance
  and language-port work remains tracked by SOW-0009 child SOWs.

## Outcome

Completed.

- Go, Node.js, and Python now document or test the same row-scoped
  current-entry facade payload lifetime as Rust.
- The product scope spec now records the cross-language facade lifetime
  contract as current reality.
- Local validation passed for Go, Node.js, Python, directory reader
  interoperability, `git diff --check`, and the SOW audit after close.
- Three read-only reviewers returned production-grade verdicts with no
  blocking findings.

## Lessons Extracted

- Row-scoped facade lifetime is best validated by retaining the exact values
  returned by enumeration and checking them only after end-of-row.
- Python tests in this repository require explicit `main()` registration.

## Followup

None from this SOW.
