# SOW-0043 - Rust Reader Libsystemd/Jf Parity

## Status

Status: completed

Sub-state: completed on 2026-05-29 after two whole-SOW reviewer rounds
returned production-grade results and all findings were resolved or
dispositioned.

## Requirements

### Purpose

Define and close the Rust reader compatibility target before reader performance
optimization or other language reader alignment. Rust must be the reader
reference only after it is audited against libsystemd behavior and Netdata's
`jf` facade needs.

### User Request

The user wants the reader phase to mirror the writer phase:

- align Rust to libsystemd and the Netdata `jf` facade;
- preserve and port the `jf` libsystemd-like reader API concept;
- then optimize Rust reader performance;
- then align Go, Python, and Node.js.

The user also agreed to fold RAW byte field-name reader representation into
this reader parity SOW.

### Assistant Understanding

Facts:

- Netdata has a `jf` crate that provides a libsystemd-like API for reader
  callers.
- SDK readers must support ordered multi-file directory reading.
- Reader APIs must represent RAW byte field names without losing information.
- Reader work must include single-file and directory readers.

Inferences:

- Rust reader parity must come before Rust reader optimization; otherwise
  optimization may target the wrong API/behavior.

Unknowns:

- Exact libsystemd reader calls required by all Netdata consumers at the final
  integration commit.
- Exact byte-preserving reader API shape across all languages.

### Acceptance Criteria

- Audit Rust reader against libsystemd file-backed behavior relevant to this
  SDK.
- Audit Rust reader against Netdata `jf` facade behavior required by known
  Netdata consumers.
- Define the shared reader API layers: idiomatic SDK reader and
  libsystemd-compatible facade.
- Define RAW byte field-name representation for Rust, Go, Node.js, and Python.
- Ensure directory readers support mixed formats in one directory:
  compression on/off, mixed compression algorithms, compact on/off, FSS on/off,
  open/closed files, and historical compatible files.
- Identify any Rust reader correctness gaps and either fix them here or split a
  concrete follow-up SOW before close.
- Update specs and docs for reader contracts.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/specs/product-scope.md`
- `.agents/sow/done/SOW-0027-20260526-netdata-reader-api-and-jf-facade.md`
- `.agents/sow/done/SOW-0024-20260526-mixed-format-directory-readers.md`
- `.agents/sow/done/SOW-0039-20260528-raw-byte-field-name-reader-representation.md`

Current state:

- Rust reader exists and supports directory reading.
- SOW-0027 already implemented the accepted `jf`/libsystemd facade subset in
  Rust, Go, Node.js, and Python. For Rust, the accepted facade surface is
  visible in `rust/src/journal/src/facade.rs`: open/open-file/open-directory/
  open-files/close at lines 71-117 and the exported stateful operations at
  lines 384-538.
- A fresh Rust audit found one concrete parity gap in the idiomatic reader
  entry surface: `Entry.payloads` preserved full `FIELD=value` bytes, but
  `Entry.fields` and `Entry.field_values` previously converted RAW field names
  with lossy UTF-8. RAW files can contain non-UTF8 field names under the
  writer's `FieldNamePolicy::Raw`, so lossy string keys could invent names that
  are not present on disk.
- RAW byte-name reader representation was originally tracked separately and is
  now folded into this SOW.

Risks:

- Optimizing Rust reader before parity may bake in an incomplete API.
- String-keyed convenience maps can lose RAW byte field-name identity unless a
  byte-preserving surface is defined.
- Adding public fields to `Entry` would create unnecessary Rust API churn for
  downstream code using struct literals. The implemented RAW reader surface is
  method-based instead.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Reader performance work needs a stable correctness target. The target is
  libsystemd-compatible file-backed behavior plus Netdata `jf` facade needs,
  not just current SDK reader behavior.

Evidence reviewed:

- SOW-0042 writer certification completed on 2026-05-29, so the prior
  activation blocker is cleared.
- Product scope reader sections.
- SOW-0027 reader API/facade history.
- SOW-0024 mixed-directory reader history.
- SOW-0039 RAW byte-name gap.

Affected contracts and surfaces:

- Rust reader API.
- Cross-language reader API model.
- `jf`/libsystemd-compatible facades.
- Directory readers, query, unique/facet scans, cursors, seek behavior,
  journalctl rewrites, and Netdata integration readiness.

Existing patterns to reuse:

- Existing Rust `DirectoryReader`.
- Existing shared fixtures and conformance tests.
- Existing `jf` facade analysis from SOW-0027.

Risk and blast radius:

- High. This defines the reader reference for all other languages and Netdata
  reader integrations.

Sensitive data handling plan:

- Use generated or public fixtures only. Do not record real customer logs,
  SNMP communities, credentials, bearer tokens, personal data, private
  endpoints, or production incident details.

Implementation plan:

1. Inventory libsystemd and `jf` reader calls relevant to this SDK.
2. Audit Rust reader behavior against that inventory.
3. Design byte-preserving RAW field-name representation.
4. Fix or track correctness gaps.
5. Update specs/docs/tests.

Validation plan:

- Rust reader tests.
- Shared reader conformance and mixed-directory tests.
- Cross-language fixture readback where relevant.
- Read-only reviewer passes.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: update compatibility skill if reader workflow changes.
- Specs: update reader contract.
- End-user/operator docs: update reader API docs.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: close before reader optimization SOWs.
- SOW-status.md: update when activated and closed.

Open-source reference evidence:

- systemd/libsystemd evidence must be collected during implementation and
  cited as owner/repo plus commit and repository-relative paths.

Open decisions:

- None. The byte-preserving Rust reader API was implemented as additive
  methods to avoid public `Entry` struct field churn.

## Implications And Decisions

- 2026-05-28: user agreed RAW byte field-name representation folds into reader
  parity instead of remaining a standalone SOW.

## Plan

1. Activate after writer closure or explicit user reprioritization.
2. Complete Rust reader parity audit.
3. Fix or track correctness gaps.
4. Update specs and docs.

## Delegation Plan

Implementer:

- Local implementation by the project manager unless the user changes routing.

Reviewers:

- Read-only reviewers from the approved pool after the whole SOW is locally
  implemented and validated.

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

- Record parity gaps, user decisions, reviewer findings, and audit failures in
  this SOW before moving to performance work.

## Execution Log

### 2026-05-28

- Created from the agreed SOW restructuring.

### 2026-05-29

- Activated after SOW-0042 completed and the writer phase moved to reader
  work.
- Loaded project journal-compatibility and agent-orchestration skills.
- Confirmed the new external-review cadence: complete the whole SOW locally
  first, then run read-only reviewers against the whole SOW and changed
  surface.
- Audited the current Rust facade against SOW-0027's accepted `jf`/libsystemd
  subset. No missing Rust facade operation was found for this SOW's file-backed
  target.
- Fixed the RAW byte-name reader gap in Rust:
  - `Entry::raw_fields()`, `Entry::get_raw()`, and
    `Entry::get_raw_values()` now split canonical `payloads` without lossy
    UTF-8 conversion.
  - `Entry.fields` and `Entry.field_values` now include only valid UTF-8 field
    names.
  - Full payload bytes remain the canonical RAW reader surface.
- Updated Rust README and product scope with the RAW reader contract.
- Ran the first whole-SOW reviewer batch against Kimi, Qwen, GLM, and Minimax.
  All four reviewers classified the SOW as production-grade. Kimi raised one
  medium finding that export/JSON output silently dropped non-UTF8 RAW field
  names; the other reviewers treated the same issue as non-blocking but worth
  documenting.
- Resolved the export part of that finding by making `export_entry_bytes()`
  append non-UTF8 RAW field names from `Entry::raw_fields()` as bytes. JSON,
  field enumeration, unique queries, and `get_data` remain UTF-8 field-name
  surfaces by design and are now documented in the spec and Rust README.
- Ran the second whole-SOW reviewer batch after the export fix. Kimi, Qwen,
  GLM, and Minimax all classified the SOW as production-grade with no blocking
  findings.

## Validation

Acceptance criteria evidence:

- Rust facade parity target:
  - `rust/src/journal/src/facade.rs:71-117` provides open path, open file,
    open directory, open files, and close wrappers.
  - `rust/src/journal/src/facade.rs:384-538` provides match groups,
    next/previous/skip, seek head/tail/realtime/cursor, realtime, seqnum,
    monotonic/boot, cursor/test-cursor, entry, get data, data enumeration,
    field enumeration, boot listing, unique query/enumeration, and output
    processing.
  - `rust/src/journal/src/lib.rs:2218-2350` contains the focused facade
    stateful-operation regression test inherited from SOW-0027.
- RAW byte-name reader representation:
  - `rust/src/journal/src/lib.rs:119-190` defines borrowed `RawField` and the
    byte-preserving entry methods.
  - `rust/src/journal/src/lib.rs:1510-1518` keeps full payload bytes and avoids
    lossy string-key insertion for non-UTF8 RAW field names.
  - `rust/src/journal/src/lib.rs:2354-2410` tests invalid UTF-8 field names,
    NUL-containing field names, spaces in RAW field names, binary values
    containing `=`, repeated byte-keyed lookup, and absence of invented lossy
    string keys. The same test now asserts export byte output preserves the
    invalid UTF-8 field name and JSON does not invent a lossy field name.
- RAW export behavior:
  - `export_entry_bytes()` keeps existing UTF-8 field ordering, then appends
    non-UTF8 RAW fields from `Entry::raw_fields()` through a byte-name export
    writer.
  - JSON output, field enumeration, unique queries, and `get_data` remain
    UTF-8 field-name surfaces by design; byte-exact RAW callers use payloads,
    data enumeration, or the idiomatic byte-name API.
- Shared reader API contract:
  - `.agents/sow/specs/product-scope.md:396-403` records full `FIELD=value`
    payload bytes as canonical for RAW-mode readers and records the Rust
    method-based byte-name surface.
  - `.agents/sow/specs/product-scope.md:541-544` records the current Rust
    reader slice.
- End-user docs:
  - `rust/README.md:62-65` documents the RAW reader surface.
  - `rust/README.md:219-231` gives the byte-keyed usage example.
- Mixed-format directory support:
  - `run_directory_matrix.py --readers stock rust` passed on systemd 260
    (260.1-2-manjaro), including stock layout traversal, match OR/AND,
    `+` disjunction, export/text/fields/list-boots, corrupt-file skip,
    verify skip, repository `.journal.zst` discovery, and empty directories.
  - `run_mixed_directory_matrix.py --readers stock rust` passed 27/27 on
    systemd 260 (260.1-2-manjaro), including regular/compact, zstd/xz/lz4
    DATA compression, sealed/unsealed, active/archived names, and whole-file
    `.journal.zst` repository extension.
- File-backed query/follow behavior:
  - `run_journalctl_query_matrix.py` passed for stock, Rust, Go, Node.js, and
    Python journalctl rewrites on systemd 260 (260.1-2-manjaro), covering
    `--file`, `--directory`, `--since`, `--until`, `--boot`, and live
    `--follow` cases.
- Binary field readback:
  - `run_binary_matrix.py --writers rust --readers stock rust` passed 7/7,
    including stock verify, stock JSON/export/export-match, stock libsystemd
    helper, and Rust JSON/export readback.

Tests or equivalent validation:

- `CARGO_HOME=.local/cargo-home CARGO_TARGET_DIR=.local/cargo-target cargo test --manifest-path rust/Cargo.toml -p journal reader_preserves_raw_byte_field_names`
  - PASS, 1/1 before reviewer fixes and PASS, 1/1 after the export byte-name
    fix.
- `CARGO_HOME=.local/cargo-home CARGO_TARGET_DIR=.local/cargo-target cargo test --manifest-path rust/Cargo.toml -p journal`
  - PASS, 14/14 plus doctests.
- `CARGO_HOME=.local/cargo-home CARGO_TARGET_DIR=.local/cargo-target cargo test --manifest-path rust/Cargo.toml -p journalctl`
  - PASS, 9/9.
- `CARGO_HOME=.local/cargo-home CARGO_TARGET_DIR=.local/cargo-target cargo test --manifest-path rust/Cargo.toml`
  - PASS across the Rust workspace before and after the export byte-name fix.
- `python3 tests/interoperability/run_directory_matrix.py --readers stock rust`
  - PASS, status `PASS`, systemd 260 (260.1-2-manjaro), before and after the
    export byte-name fix.
- `PYTHON=.local/python-venv/bin/python .local/python-venv/bin/python tests/interoperability/run_mixed_directory_matrix.py --readers stock rust`
  - PASS, 27/27, systemd 260 (260.1-2-manjaro), before and after the export
    byte-name fix.
- `env GOCACHE=/home/costa/Documents/systemd-journal-sdk/.local/go-cache GOMODCACHE=/home/costa/Documents/systemd-journal-sdk/.local/go-mod-cache GOPATH=/home/costa/Documents/systemd-journal-sdk/.local/go-path CARGO_HOME=/home/costa/Documents/systemd-journal-sdk/.local/cargo-home CARGO_TARGET_DIR=/home/costa/Documents/systemd-journal-sdk/.local/cargo-target npm_config_cache=/home/costa/Documents/systemd-journal-sdk/.local/npm-cache PIP_CACHE_DIR=/home/costa/Documents/systemd-journal-sdk/.local/pip-cache PYTHONPATH=/home/costa/Documents/systemd-journal-sdk/.local/python-deps:/home/costa/Documents/systemd-journal-sdk/python PYTHON=/home/costa/Documents/systemd-journal-sdk/.local/python-venv/bin/python /home/costa/Documents/systemd-journal-sdk/.local/python-venv/bin/python tests/interoperability/run_journalctl_query_matrix.py`
  - PASS, status `PASS`, systemd 260 (260.1-2-manjaro).
- `python3 tests/interoperability/run_binary_matrix.py --writers rust --readers stock rust`
  - PASS, 7/7, systemd 260 (260.1-2-manjaro), before and after the export
    byte-name fix.
- `python3 tests/interoperability/run_mixed_directory_matrix.py --readers stock rust`
  - Failed before fixture generation with `ModuleNotFoundError: No module
    named 'lz4'` because the system Python lacks the documented
    `lz4==4.4.5` dependency. Disposition: environment setup failure only;
    rerun with `.local/python-venv/bin/python` passed 27/27.

Real-use evidence:

- No live host journal probing was performed. Real-use equivalence for this
  SOW is the stock `journalctl --file`/`--directory`, stock libsystemd helper,
  and repository journalctl matrix evidence above against repo-local generated
  fixtures.

Reviewer findings:

- Round 1 read-only whole-SOW review:
  - Kimi: PRODUCTION GRADE with one medium finding. Finding: export/JSON
    output omitted non-UTF8 RAW field names because both used the UTF-8
    `entry.field_values` map. Disposition: fixed export byte output to include
    non-UTF8 RAW field names; documented JSON and other string-name helpers as
    UTF-8 field-name surfaces.
  - Qwen: PRODUCTION GRADE. Low design notes: field enumeration/unique and
    export/JSON were UTF-8-only, and `get_raw()` is O(n). Disposition: fixed
    export; documented JSON/string helpers; `get_raw()` optimization remains
    SOW-0044.
  - GLM: PRODUCTION GRADE. Non-blocking observations: output formatting
    limits and `get_raw()` linear scans. Disposition: same as above.
  - Minimax: PRODUCTION GRADE with the same informational output-formatting
    note and SOW hygiene note that Outcome/Lessons were pending. Disposition:
    export fixed, docs/specs updated, terminal sections populated below.
- Round 2 read-only whole-SOW review after fixes:
  - Kimi: PRODUCTION GRADE. No blocking findings. Confirmed export byte output
    preserves non-UTF8 RAW names, JSON/string helpers remain bounded, docs and
    SOW hygiene are correct, and the SOW can close.
  - Qwen: PRODUCTION GRADE. No blocking findings. Low observations:
    field-enumeration/unique are UTF-8-only by design and `export_entry()`
    string conversion is lossy for non-UTF8 bytes. Disposition: documented
    boundary is sufficient; byte-preserving APIs are available.
  - GLM: PRODUCTION GRADE. No blocking findings. Low observation: redundant
    `_BOOT_ID` defensive filter in the non-UTF8 export path. Disposition:
    harmless defensive check; no change needed.
  - Minimax: PRODUCTION GRADE. No blocking findings. Confirmed code, tests,
    docs/specs, SOW evidence, and repository boundary are clean.

Same-failure scan:

- `rg -n "raw_fields|raw_field_values|get_raw" .agents/sow rust go node python tests`
  found only the new Rust API/docs/spec entries plus unrelated writer-test
  variable names. No stale `raw_field_values` reader API reference remains.

Sensitive data gate:

- PASS. Changes use generated fixture field names and values only. No customer
  data, credentials, tokens, private endpoints, SNMP communities, or personal
  data were added.

Artifact maintenance gate:

- `AGENTS.md`: no update needed; workflow did not change.
- Runtime project skills: no update needed; compatibility and orchestration
  rules already cover this work.
- Specs: updated `.agents/sow/specs/product-scope.md` with RAW reader
  representation.
- End-user/operator docs: updated `rust/README.md`.
- End-user/operator skills: no output/reference skill affected.
- SOW lifecycle: completed and moved to `.agents/sow/done/` after reviewer
  closeout.
- `SOW-status.md`: updated when SOW-0043 was activated.

Specs update:

- Updated `.agents/sow/specs/product-scope.md` for RAW byte-name entry access,
  export byte output, and UTF-8-only JSON/string facade helper boundaries.

Project skills update:

- No project skill update needed. The existing compatibility skill already
  requires byte-preserving reader support, mixed-directory tests, and
  libsystemd-style facade parity.

End-user/operator docs update:

- Updated `rust/README.md` for RAW byte-name entry access, export byte output,
  and UTF-8-only JSON/string facade helper boundaries.

End-user/operator skills update:

- No output/reference skill affected.

Lessons:

- Keep RAW reader identity byte-first. String-keyed convenience maps must never
  synthesize lossy replacement names.
- Prefer method-based byte views over adding public `Entry` fields when the
  existing struct shape is already public.
- Output helpers are not all equivalent. Export is byte output and can preserve
  non-UTF8 RAW names; JSON, field enumeration, unique queries, and `get_data`
  are string-name surfaces and must be documented as such.

Follow-up mapping:

- Go, Node.js, and Python byte-name reader API alignment remains tracked by
  the planned reader alignment SOWs: SOW-0045 and SOW-0046.
- Rust reader performance work remains SOW-0044.
- Netdata integration remains SOW-0026 and should stay after reader
  performance gates as previously decided.

## Outcome

Rust reader parity is implemented, validated, and reviewed. The Rust reader now
has an additive byte-preserving RAW field-name surface, avoids lossy UTF-8
field-name invention, preserves non-UTF8 RAW names in export byte output, and
documents the remaining UTF-8-only string helper boundaries. The SOW passed
two whole-SOW reviewer rounds and is ready for SOW-0044.

## Lessons Extracted

- RAW reader identity must be defined around byte payloads first; string maps
  are convenience views only.
- Additive methods are safer than new public fields when a public Rust struct
  already exists.
- Reviewer batching worked better at whole-SOW scope: all reviewers converged
  on the same output-formatting boundary, which was meaningful enough to fix
  before close.

## Followup

None yet.

## Regression Log

None yet.
