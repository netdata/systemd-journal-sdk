# SOW-0021 - Node.js XZ DATA Compression

## Status

Status: completed

Sub-state: completed after retry-path repair, full validation, production-grade reviewer round 4, artifact cleanup, and SOW audit.

## Requirements

### Purpose

Complete the remaining Node.js XZ-compressed DATA object support while preserving stock systemd journal compatibility and the repository's no-systemd-journal-library boundary.

### User Request

The user requires all writer implementations to support systemd-defined journal compression where applicable and to use current common compression libraries rather than manually implementing compression algorithms when suitable libraries exist.

### Assistant Understanding

Facts:

- SOW-0017 implements Rust/Go XZ+LZ4, Python XZ+LZ4, and Node.js LZ4.
- Node.js XZ remains unsupported in SOW-0017.
- The Node.js writer API is currently synchronous.
- `node-liblzma@5.0.1` was the latest checked npm XZ/LZMA2 candidate during SOW-0017 Phase 2B.
- `node-liblzma@5.0.1` default package metadata includes native addon dependencies; its non-native WASM API is async.

Inferences:

- Node.js XZ likely requires either a small synchronous API extension, an async compression path that does not disrupt existing writer calls, or a different current package that provides synchronous `.xz`/LZMA2 `CHECK_NONE` output without native addons.

Unknowns:

- Whether a current Node.js compression package can synchronously write systemd-compatible `.xz` streams without native addons.
- Whether the user will accept a Node.js async writer API extension specifically for XZ.

### Acceptance Criteria

- Node.js reader can read XZ-compressed DATA objects written by Rust, Go, Python, stock-compatible fixtures, and Node.js if writing is implemented.
- Node.js writer can write XZ-compressed DATA objects, or the SOW records a user decision accepting an API/policy limitation.
- Written Node.js XZ journals pass stock `journalctl --verify --file`, stock journalctl JSON/export reads, stock libsystemd reads, and repository Rust/Go/Python/Node readers where supported.
- Dependency review records latest stable package versions and why the selected path is acceptable.

## Analysis

Sources checked:

- `.agents/sow/current/SOW-0017-20260524-xz-lz4-data-writing.md`
- `node/src/lib/writer.js`
- `node/src/lib/entry.js`
- npm metadata for `node-liblzma@5.0.1`

Current state:

- Node.js LZ4 DATA object reading/writing is implemented in SOW-0017.
- Node.js XZ DATA objects are still rejected.

Risks:

- Converting the writer API to async could affect all Node.js writer callers.
- Native addon usage would violate the current Node.js project constraint unless the user explicitly changes it.
- WASM packaging may add runtime initialization and deployment complexity.

## Pre-Implementation Gate

Status: ready for feasibility pass; implementation must stop if a product/API policy decision is required.

Problem / root-cause model:

- systemd XZ DATA objects require `.xz` streams using LZMA2 and `CHECK_NONE`.
- The current Node.js writer path is synchronous, but the checked non-native package path for XZ is async WASM.

Evidence reviewed:

- `node/src/lib/writer.js`: synchronous `Writer.create()` and `append()` API.
- `node/src/lib/entry.js`: XZ DATA objects remain unsupported after SOW-0017 Phase 2B.
- `node-liblzma@5.0.1` npm metadata: default package includes native addon dependencies; WASM path is non-native but async.

Affected contracts and surfaces:

- Node.js writer API.
- Node.js reader DATA decompression.
- Node.js package dependencies and packaging.
- Shared compression matrix.
- README and product-scope specs.

Existing patterns to reuse:

- SOW-0017 Node.js LZ4 helper style in `node/src/lib/lz4-block.js`.
- Shared `tests/interoperability/run_compression_matrix.py`.
- Existing synchronous writer option parsing.

Risk and blast radius:

- Medium. Reader support is isolated, but writer support may require API or package-policy decisions.

Sensitive data handling plan:

- Use synthetic compression fixtures only. Record package metadata and validation commands; no secrets or customer data.

Implementation plan:

1. Re-check latest Node.js XZ/LZMA2 package options and licenses.
2. Present a user decision if support requires async writer API changes or native addon policy changes.
3. Implement Node.js XZ reader/writer support after the decision.
4. Extend compression matrix coverage for Node.js XZ.

Validation plan:

- Node package tests.
- Node.js XZ compression matrix against stock journalctl, stock libsystemd, Rust, Go, Python, and Node readers.
- zstd/lz4 regression matrices remain passing.
- External reviewer pass.

Artifact impact plan:

- AGENTS.md: no update expected unless dependency/native-addon policy changes.
- Runtime project skills: update only if Node.js XZ adds a durable workflow.
- Specs: update Node.js reader/writer support slice.
- End-user/operator docs: update Node README and interoperability README.
- End-user/operator skills: none expected.
- SOW lifecycle: split from SOW-0017, then activation and completion in this SOW.
- SOW-status.md: update during activation and close.

Open-source reference evidence:

- No external source repository was checked yet. Package metadata was enough to identify the current decision point.

Open decisions:

- Decide whether Node.js XZ may add an async writer path, use a native addon compression package, or remain unsupported until a synchronous non-native package is found. The implementer must present concrete evidence/options and stop before making a product/API policy change if no synchronous non-native path exists.

## Implications And Decisions

1. SOW-specific implementer model experiment
   - Decision: use `deepseek/deepseek-v4-pro` with opencode as the implementer for this SOW.
   - Reason: the user wants to evaluate DeepSeek as an implementation model.
   - Implication: this does not change the project-wide preferred implementer model. Kimi remains the default implementer for subsequent SOWs unless the user changes the global routing.
   - Risk: if DeepSeek produces weak code or ignores repository/SOW boundaries, the project manager must stop the run, record the failure, and either request another user decision or revert to the normal implementer routing for this SOW.

2. SOW-specific opencode runtime metadata exception
   - Decision: use the user's normal opencode environment for the DeepSeek Pro implementer run.
   - Reason: forcing repo-local `XDG_DATA_HOME` and `XDG_CACHE_HOME` isolated opencode from its normal auth/model registry, which made `deepseek/deepseek-v4-pro` appear unavailable even though it is available in the user's normal opencode environment.
   - Scope: this exception is limited to opencode's own runtime metadata/auth/session state for this SOW-specific DeepSeek Pro experiment.
   - Boundary retained: SDK code changes, generated project artifacts, scratch work, package caches, dependency downloads, and delegated agent file edits must stay inside this repository or `/tmp`.
   - Risk: opencode may create or update runtime metadata outside the repository while executing the implementer run.

3. Node.js XZ dependency policy clarification

Decision: choose option B and accept `node-liblzma@5.0.1` as the Node.js XZ dependency.
Clarified requirement: the Node.js SDK must not load or link native code at runtime. A dependency package may ship native artifacts if the SDK runtime path is constrained and tested to use only the WASM implementation.
Reason: the package-managed dependency keeps maintainability, update visibility, lockfile integrity, scanner visibility, and upstream provenance. Vendoring the WASM artifacts would reduce maintainability and can create a false sense of security because the WASM remains compiled compression code that still needs advisory tracking and updates.
Boundary: Node.js XZ code must import/use the WASM path only; it must not import the default native entrypoint or load `.node` files.
Required validation: add or preserve a guard/test proving the runtime path is WASM-only and that emitted XZ streams use `CHECK_NONE`.
Risk accepted: dependency scanners and strict environments may flag native artifacts, `hasInstallScript: true`, `node-gyp-build`, and `.node` files in `node-liblzma` even though this SDK does not load them at runtime.

## Plan

1. Dependency/API feasibility review.
2. User decision if required.
3. Implementation and shared matrix validation.
4. Review and commit.

## Delegation Plan

Implementer:

- SOW-specific implementer for this experiment is `deepseek/deepseek-v4-pro` via opencode normal coding mode.
- Project default implementer remains `llm-netdata-cloud/kimi-k2.6` for other SOWs.

Reviewers:

- `llm-netdata-cloud/minimax-m2.7-coder`, `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`, and `llm-netdata-cloud/glm-5.1`.

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

- Record implementer failure, reviewer failure, audit failure, dependency rejection, or model unavailability before changing plan or model.

## Execution Log

### 2026-05-24

- Created as a split from SOW-0017 to track Node.js XZ DATA object support.

### 2026-05-24 - Activation And DeepSeek Experiment

- Activated after user requested testing `deepseek/deepseek-v4-pro` as the implementer for this SOW.
- Recorded that DeepSeek is a SOW-specific implementer experiment only; global project routing remains unchanged.
- The implementer prompt must allow dependency/API feasibility investigation and implementation only if no unresolved product/API policy decision is required.
- First opencode run with repo-local `XDG_DATA_HOME` and `XDG_CACHE_HOME` failed before implementation with `Model not found` for `deepseek/deepseek-v4-pro`; opencode suggested model id `deepseek-v4-pro`.
- Retry with `deepseek-v4-pro` under the same repo-local XDG isolation also failed before implementation with `Model not found: deepseek-v4-pro/`.
- Availability check under repo-local XDG isolation showed only `opencode/deepseek-v4-flash-free` for DeepSeek and no `deepseek/deepseek-v4-pro`.
- Root cause: repo-local XDG isolation prevented opencode from seeing the user's normal opencode auth/model registry.
- Verification without repo-local XDG isolation showed `deepseek/deepseek-v4-pro` is available in the user's normal opencode environment.
- The user approved option 1: use normal opencode runtime state for this SOW-specific experiment while retaining the repository boundary for all project edits, generated project artifacts, scratch work, and dependency caches.
- DeepSeek Pro implementer round 1 identified `lzma-wasm@1.0.7` as a candidate and began partial implementation.
- Round 1 was interrupted by the project manager before acceptance because it misread the XZ stream flags. The generated stream header began `fd377a585a000004...`; `xz --robot --list` reported `CRC64`, not `None`.
- Compatibility evidence from `systemd/systemd @ cf3156842209`, `src/basic/compress.c:208`: systemd writes journal XZ blobs with `LZMA_CHECK_NONE`. The next implementer pass must not accept an XZ writer path unless it can emit CHECK_NONE streams or records a user decision changing that requirement.

### 2026-05-24 - DeepSeek Pro Round 2: Successful Implementation

#### Dependency Review

- Rejected `lzma-wasm@1.0.7` (wasm, no `check` parameter, always emits CRC64): confirmed via header `fd377a585a000004` + `xz --robot --list` → CRC64.
- Rejected `compress-utils@0.7.1` (async XZ API, level 1-10 only, no check control, includes native `.node` addon prebuilds).
- Rejected `wasm-xz-sys@1.1.0` (CLI wrapper around xz binary, not a library API).
- Rejected `node-liblzma/inline` WASM path (inline WASM binary corrupted/different from standalone `liblzma.wasm`; `LinkError` on Node.js v22).
- **Accepted `node-liblzma@5.0.1` standalone WASM path**:
  - Uses `lib/wasm/liblzma.wasm` (107KB) + `lib/wasm/liblzma.js` (Emscripten glue) loaded via `readFileSync` + `createLZMA({ wasmBinary })`.
  - Raw Emscripten API: `_lzma_easy_buffer_encode` with `LZMA_CHECK_NONE=0` for compression, `_lzma_stream_buffer_decode` for decompression.
  - No native addon, no CGO, no system journal library. WASM-only runtime path.
  - License: LGPL-3.0+ (liblzma source) + MIT (JS bindings).
  - WASM init via top-level await: module loads `liblzma.wasm` once at import time; exported compress/decompress functions are synchronous.
  - liblzma version in WASM binary: 5.8.3.

#### CHECK_NONE Verification

- Generated XZ stream header: `fd377a585a000000ff12d941` (stream flags bytes 6-7 are `00 00`; byte 7 check type `00` = CHECK_NONE).
- `xz --robot --list /tmp/test-xz-none2.xz` reported check: `None`.
- Round-trip compress/decompress verified.

#### Implementation Changes

Files modified:
1. `node/package.json` — removed `lzma-wasm@^1.0.7`, added `node-liblzma@5.0.1`.
2. `node/src/lib/xz-block.js` — rewritten to use `node-liblzma` standalone WASM with CHECK_NONE; synchronous compress/decompress via top-level await init.
3. `node/src/lib/writer.js`:
   - `normalizeCompression()`: added `COMPRESSION_XZ / 'xz'` case.
   - `_initialize()`: added `INCOMPATIBLE_COMPRESSED_XZ` flag branch.
   - `_addData()`: added XZ compression branch (before LZ4, threshold >= 80 bytes).
   - `_readDataPayload()`: added XZ decompression via `decompressXzDataPayload()`.
   - `open()`: added `INCOMPATIBLE_COMPRESSED_XZ` to `supportedWriterIncompatible`.
   - `open()` compression detection: added XZ check (highest priority, before LZ4/ZSTD).
4. `node/src/lib/entry.js` — added `import decompressXzDataPayload`; replaced XZ rejection with decompression branch.
5. `node/src/lib/reader.js` — added `INCOMPATIBLE_COMPRESSED_XZ` to supported flags mask and import.
6. `node/src/lib/lz4-block.js` — no changes.
7. `node/test/all.js`:
   - Updated unsupported-flags test to use bit 4 (not XZ) for rejection test.
   - Updated unsupported-flags DATA test to use bit 7 (not XZ).
   - Added XZ round-trip compression test via `parseDataObject()`.
   - Added `compressXzDataPayload`/`decompressXzDataPayload` imports.
8. `node/internal/testcmd/livewriter.js` — added `--xz-fixture` flag and fixture field generation.
9. `tests/interoperability/README.md` — updated compression support rows: xz complete after SOW-0021; lz4 complete from SOW-0017 with Python `lz4` dependency required for Python reader/writer validation.

#### Validation Results

- `node test/all.js`: PASS (all tests including new XZ round-trip).
- `run_compression_matrix.py --writers go rust node python --readers stock go rust node python --compression xz`: 72/72 PASS.
  - Stock `journalctl --verify --file`: PASS for Node.js XZ journal.
  - Stock `journalctl --file --output=json/export`: PASS.
  - Stock libsystemd reader: PASS.
  - Go/Rust/Node/Python cross-language reads: PASS.
- `run_compression_matrix.py --writers node --compression zstd`: 18/18 PASS (no regression).
- `run_compression_matrix.py --writers go node --compression lz4` initially failed for Python reads in the local shell that lacked the Python `lz4` dependency path. Later validation with the SOW-0017 dependency setup (`PYTHONPATH=$PWD/.local/python-deps:$PWD/python`) passed 72/72, so this was an environment dependency issue, not a Python LZ4 reader compatibility gap.
- Project manager note: the implementer marked this SOW completed before independent review and audit. Treat the implementation result as provisional until the reviewer gate and audit are complete.

### 2026-05-24 - Reviewer Gate Round 1

- Reviewer models run read-only: `llm-netdata-cloud/glm-5.1`, `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`, and `llm-netdata-cloud/minimax-m2.7-coder`.
- Completed usable verdicts:
  - GLM: NOT PRODUCTION GRADE.
  - Mimo: NOT PRODUCTION GRADE.
  - Minimax: NOT PRODUCTION GRADE.
- Qwen run did not return a final usable verdict before the session became unavailable, so it is not counted for the gate.
- Common blocking finding: `node-liblzma@5.0.1` is a native-addon distribution even though the implementation imports only its WASM files. Evidence:
  - `node/package.json` adds `node-liblzma`.
  - `node/package-lock.json` records `node-addon-api`, `node-gyp-build`, `hasInstallScript: true`, and `license: LGPL-3.0`.
  - installed package evidence shows `prebuilds/*/*.node`, `binding.gyp`, `build/Release/node_lzma.node`, and `postinstall: node-gyp-build`.
- Common technical findings to address if this implementation path continues:
  - `node/src/lib/xz-block.js` top-level WASM initialization fails the entire module on missing/corrupt dependency.
  - `_malloc()` return values are not checked before writing into WASM memory.
  - decompression buffer growth is not capped against `MAX_UNCOMPRESSED_DATA_OBJECT_SIZE`.
  - `reader.js` comment still says XZ is rejected.
  - invalid decompression errors are double-wrapped.
  - XZ helper has an effective 80-byte minimum while the writer threshold defaults to 64.
- Documentation/spec findings:
  - `.agents/sow/specs/product-scope.md` still says Node.js rejects XZ DATA objects and cannot write XZ.
  - `tests/interoperability/README.md` marks xz/lz4 writer parity complete despite known Python LZ4 reader failures; the XZ completion claim is premature while the SOW remains in review.
- Reviewer side-effect note: Minimax violated the read-only reviewer prompt by running `npm install --ignore-scripts`. The project manager must inspect any resulting working-tree side effects before continuing.
- User decision: choose option B. Accept `node-liblzma@5.0.1` as a package-managed dependency because the runtime path is WASM-only and maintainability/transparency are better than vendoring. The "no native" requirement is clarified to mean no native runtime loading/linking by the SDK, not no native artifacts anywhere in dependency packages.

### 2026-05-25 - Repair Pass After Reviewer Gate Round 1

#### Repairs Applied

1. **Kept `node-liblzma@5.0.1`** with WASM-only runtime path:
   - Import uses only exported WASM subpaths `node-liblzma/wasm/liblzma.js` and `node-liblzma/wasm/liblzma.wasm`, resolved through Node package resolution.
   - No default native entrypoint import. No `.node` file loading.

2. **Hardened `node/src/lib/xz-block.js`**:
   - Added `checkedMalloc()` function that verifies `_malloc()` return value is non-zero before writing into WASM memory.
   - Capped decompression output-buffer growth: if `outSize * 4 > MAX_UNCOMPRESSED_DATA_OBJECT_SIZE`, throws immediately instead of growing unbounded.
   - Removed outer try/catch wrapper in `decompressXzDataPayload()` that double-wrapped error messages. Errors now propagate with clear messages.
   - CHECK_NONE is produced via `LZMA_CHECK_NONE = 0` (verified by XZ stream flags byte 7 exactly `0x00`).

3. **Fixed stale `reader.js` comment**: line 62 now says `// Reject flags we cannot handle (compact)` (removed `xz`).

4. **Clarified XZ minimum threshold behavior**: `compressXzDataPayload()` returns null for payloads < 80 bytes. Writer `compressThreshold` defaults to 64; payloads between 64-79 bytes pass the writer threshold but the XZ helper returns null, causing fallback to uncompressed. This is consistent with LZ4 behavior (returns null for payloads < 9 bytes).

5. **Added/extended tests in `node/test/all.js`**:
   - CHECK_NONE test: verifies XZ stream magic bytes 0-5 and stream flags bytes 6-7 are both zero, including byte 7 CHECK_NONE.
   - XZ minimum threshold test: verifies payload < 80 bytes returns null.
   - Invalid/corrupt XZ test: verifies garbage payload throws `xz decompression` error.
   - Runtime guard test: uses `createRequire` to check `require.cache` for `node-liblzma` `.node` paths, asserts none found.
   - Writer → Reader XZ round-trip: creates journal with `compression: 'xz'`, writes a large compressible entry, verifies at least one DATA object has the XZ compression flag, reads back, and verifies decompressed value.

6. **Updated durable artifacts**:
   - `.agents/sow/specs/product-scope.md`: Node.js XZ reader/writer is current reality. Native policy clarified to "no native runtime loading/linking; dependency packages may ship native artifacts if SDK runtime path uses only non-native implementations".
   - `.agents/skills/project-journal-compatibility/SKILL.md`: native addon policy wording updated; bad practices and validation checklist aligned.
   - `AGENTS.md`: success criteria and project-specific overrides updated with clarified native policy.
   - `tests/interoperability/README.md`: split `xz/lz4 writer parity` row into separate xz and lz4 rows; final cleanup marks both complete and records Python LZ4 as dependency-backed, not a compatibility gap.
   - `SOW-status.md`: updated to reflect repair pass.

7. **Added `.gitignore` coverage**: `node_modules/` added before Go build artifacts entry.

8. **Project manager cleanup after implementer pass**:
   - Replaced hardcoded `../../node_modules` WASM path with `createRequire(import.meta.url).resolve(...)`, so npm dependency hoisting does not break users.
   - Corrected the CHECK_NONE unit test to inspect stream flags byte 7, not byte 6 bits.
   - Strengthened the writer/reader round-trip test to require an actually XZ-compressed DATA object on disk.
   - Normalized `node/package-lock.json` tarball URLs to official `registry.npmjs.org` URLs and verified npm metadata/integrity for `node-liblzma@5.0.1`, `node-addon-api@8.8.0`, and `node-gyp-build@4.8.4`.
   - Added rejection of invalid DATA objects that set more than one known compression flag, plus a unit test for the XZ+LZ4 flag combination.
   - Inspected the reviewer side effect from Minimax's forbidden `npm install --ignore-scripts`: resulting `node/node_modules/` remains untracked and is ignored by `.gitignore`; no durable tracked file outside this SOW scope was found from that side effect.

#### Validation Results (Repair Pass)

- `node test/all.js`: PASS after project manager cleanup (all tests including corrected CHECK_NONE, invalid multi-compression-flag rejection, threshold, corrupt payload, native-addon guard, and actual compressed-DATA round-trip tests).
- `run_compression_matrix.py --writers go rust node python --readers stock go rust node python --compression xz`: 72/72 PASS after cleanup. Result file: `.local/interoperability/compression-matrix-results-20260525-002827.json`.
  - Stock `journalctl --verify --file`: PASS for all writers.
  - Stock `journalctl --file --output=json/export`: PASS.
  - Stock libsystemd: PASS.
  - Go/Rust/Node/Python cross-language reads: PASS.
- `run_compression_matrix.py --writers node --readers stock go rust node python --compression zstd`: 18/18 PASS after cleanup (no regression). Result file: `.local/interoperability/compression-matrix-results-20260525-002826.json`.
- `.agents/sow/audit.sh`: clean after cleanup (initialization complete and clean, 0 sensitive-data findings, 0 status mismatches).

#### Artifact Changes Summary

Files modified (functional):
- `node/src/lib/xz-block.js` — package-resolution-based WASM loading, malloc checks, output cap, fixed error wrapping
- `node/src/lib/reader.js` — stale XZ rejection comment removed
- `node/test/all.js` — added 5 new test blocks

Files modified (durable artifacts):
- `.agents/sow/specs/product-scope.md` — Node.js XZ support + native policy
- `.agents/skills/project-journal-compatibility/SKILL.md` — native policy alignment
- `AGENTS.md` — native policy alignment in Goals and project-specific overrides
- `tests/interoperability/README.md` — accurate xz/lz4 status
- `SOW-status.md` — repair pass status
- `.gitignore` — added `node_modules/`

### 2026-05-25 - Reviewer Gates Round 2 Through Round 4

Round 2:

- Reviewer models run read-only: `llm-netdata-cloud/glm-5.1`, `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`, and `llm-netdata-cloud/minimax-m2.7-coder`.
- All counted reviewers returned `PRODUCTION GRADE`.
- Non-blocking observations: top-level await fails fast if the WASM dependency is missing, native artifacts and `hasInstallScript: true` remain an accepted install-time/dependency-scanner risk under option B, exact decompression bomb boundary tests are not present, and compression failures fall back to uncompressed.
- Disposition: accepted as non-blocking because the runtime path is WASM-only, validation covers stock and cross-language readers, and optional compression fallback matches existing writer behavior.

Round 3:

- Reviewer models run read-only with the same full scope after removing an extra copy from XZ output Buffer ownership.
- All reviewers returned `PRODUCTION GRADE`.
- Qwen found one low-risk allocator cleanup issue in `node/src/lib/xz-block.js`: if `checkedMalloc(outSize)` threw after freeing the old decompression output buffer during a retry, `finally` could still see the old freed pointer.
- Disposition: fixed by setting `outPtr = 0` immediately after `wasmModule._free(outPtr)` and before reallocating.

Round 4:

- Reviewer models run read-only with the same full scope plus the retry-path fix note: `llm-netdata-cloud/glm-5.1`, `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`, and `llm-netdata-cloud/minimax-m2.7-coder`.
- All four returned `PRODUCTION GRADE`.
- Non-blocking observations: top-level await import-time failure is acceptable for a required dependency, compression errors return null and fall back to uncompressed, `node-liblzma` install-time native build artifacts remain an accepted option-B risk, LGPL-3.0 should remain visible in dependency metadata, and explicit huge decompression-bomb tests are absent but the JavaScript cap and liblzma memlimit are in place.
- Disposition: no code changes required after round 4.

### 2026-05-25 - Final Validation And Artifact Cleanup

- `node test/all.js`: PASS.
- `python3 tests/interoperability/run_compression_matrix.py --writers go rust node python --readers stock go rust node python --compression xz`: 72/72 PASS. Result file: `.local/interoperability/compression-matrix-results-20260525-004515.json`.
- `python3 tests/interoperability/run_compression_matrix.py --writers node --readers stock go rust node python --compression zstd`: 18/18 PASS. Result file: `.local/interoperability/compression-matrix-results-20260525-004518.json`.
- `PYTHONPATH=$PWD/.local/python-deps:$PWD/python python3 tests/interoperability/run_compression_matrix.py --writers rust go python node --readers stock rust go python node --compression lz4 --entries 5`: 72/72 PASS. Result file: `.local/interoperability/compression-matrix-results-20260525-005308.json`.
- `python3 tests/interoperability/run_compression_matrix.py --writers rust go python node --readers stock rust go python node --compression lz4 --entries 5`: failed because the local shell lacked the Python `lz4` dependency path (`ModuleNotFoundError: No module named 'lz4'`). Disposition: not a compatibility gap; Python README and requirements already require `lz4==4.4.5`, and the dependency-backed validation above passes 72/72.
- `.agents/sow/audit.sh`: clean before close while SOW was still in current.
- `.agents/sow/audit.sh`: clean after moving completed SOW to `.agents/sow/done/`; SOW status/directory consistency passed.

## Validation

Status: complete.

Acceptance criteria evidence:

- Node.js reader reads XZ-compressed DATA objects written by Go, Rust, Node.js, and Python: PASS (`run_compression_matrix.py` XZ 72/72, result `.local/interoperability/compression-matrix-results-20260525-004515.json`).
- Node.js writer writes XZ-compressed DATA objects with CHECK_NONE: PASS (`node/test/all.js` verifies actual XZ-compressed DATA objects and XZ stream flags byte 7 `0x00`; reviewer GLM also verified generated XZ stream `fd377a585a00` with check byte `0`).
- Node.js XZ journals pass stock `journalctl --verify --file`: PASS in the XZ matrix.
- Node.js XZ journals pass stock journalctl JSON/export reads: PASS in the XZ matrix.
- Node.js XZ journals pass stock libsystemd reads: PASS in the XZ matrix.
- Node.js XZ journals pass repository Go/Rust/Node.js/Python readers: PASS in the XZ matrix.
- Dependency review: `node-liblzma@5.0.1` standalone WASM runtime path, no native addon loaded by the SDK at runtime, no CGO, no system journal library. The package ships native artifacts, has `hasInstallScript: true`, and is `LGPL-3.0`; this is accepted by the user's option B decision and documented in specs/skills.
- Existing LZ4/zstd/uncompressed behavior: Node zstd regression 18/18 PASS; all-language LZ4 matrix 72/72 PASS when Python `lz4==4.4.5` dependency path is active; uncompressed behavior untouched.
- Tests added: XZ round-trip, CHECK_NONE byte test, minimum-threshold null return, corrupt payload rejection, native-addon cache guard, actual on-disk XZ DATA object verification, and multi-compression-flag rejection.

Reviewer findings and dispositions:

- Round 1 `NOT PRODUCTION GRADE` blockers were fixed: package policy clarified by user decision B, WASM path resolved through package exports, malloc checks added, decompression output cap added, stale reader comment fixed, double-wrapped errors removed, docs/specs corrected, lockfile URLs normalized, and real compressed-DATA tests added.
- Round 2 `PRODUCTION GRADE` non-blocking findings were accepted or documented.
- Round 3 `PRODUCTION GRADE` low allocator finding was fixed with `outPtr = 0` after freeing the old retry buffer.
- Round 4 `PRODUCTION GRADE` had no blocking findings. Remaining notes are documented tradeoffs or existing validation boundaries.

Same-failure search results:

- Searched and aligned reader and writer DATA compression flag validation in `node/src/lib/entry.js` and `node/src/lib/writer.js`.
- Verified `node-liblzma` imports are only WASM subpaths in `node/src/lib/xz-block.js`; no default native entrypoint import exists.
- Verified LZ4 matrix failures without `PYTHONPATH` were due to missing local Python dependency setup, not an SDK reader bug.

Sensitive data gate:

- Durable artifacts record only public package metadata, source file paths, command names, synthetic fixture evidence, and generated result filenames. No raw secrets, credentials, private endpoints, customer identifiers, or personal data were added.

Artifact maintenance gate:

- `AGENTS.md`: updated native policy wording in Goals and project-specific overrides.
- Runtime project skills: `project-journal-compatibility` native addon policy wording, bad practices, and validation checklist updated.
- Specs: `product-scope.md` updated for Node.js XZ reader/writer support and native policy.
- End-user/operator docs: `tests/interoperability/README.md` updated with accurate xz/lz4 completion state.
- End-user/operator skills: none affected; this repo has no output/reference skill for journal SDK consumers.
- SOW lifecycle: SOW-0021 status set to `completed` and moved to `done/` at close.
- `SOW-status.md`: updated to remove SOW-0021 from current and add it to done.

SOW status/directory consistency:

- Final audit was run after moving the completed SOW to `.agents/sow/done/`; it passed with current SOW directory empty and SOW-0021 listed as completed under done.

Spec update:

- `product-scope.md` reflects Node.js XZ current support and the clarified Node.js native runtime policy.

Project skill update:

- `project-journal-compatibility` reflects the clarified Node.js native runtime policy and dependency-artifact exception.

Lessons extracted:

- Recorded below.

Follow-up mapping:

- No new SOW-0021 follow-up is required. Existing pending SOWs remain: compact journal format (SOW-0018), FSS (SOW-0019), directory traversal parity (SOW-0020), and final benchmark/profile/optimization (SOW-0009).

## Outcome

SOW-0021 is completed. Node.js now reads and writes systemd journal XZ-compressed DATA objects using `node-liblzma@5.0.1` through a package-resolved WASM-only runtime path. The implementation preserves the synchronous Node writer API after module initialization, emits CHECK_NONE XZ streams, rejects invalid multi-compression DATA flags, and passes stock journalctl, stock libsystemd, and all repository reader checks across the XZ matrix.

## Lessons Extracted

- `lzma-wasm@1.0.7` (WASM) does not expose a `check` parameter; its XZ output always uses CRC64 and cannot produce CHECK_NONE without post-processing. It is usable as a reader dependency but not as a writer dependency for systemd journal compatibility.
- `node-liblzma@5.0.1` produces CHECK_NONE XZ streams via its WASM backend, but its Node.js loading paths are fragile: the default Node.js path uses native addons (policy violation), the inline WASM path is corrupted, and the standalone WASM path works but requires manual `readFileSync` + `wasmBinary` loading.
- Top-level await is an effective pattern for async WASM initialization while keeping exported API functions synchronous.
- The Emscripten `setValue(ptr, value, 'i64')` is not implemented in newer WASM_BIGINT builds; 64-bit values must be written directly to HEAP32 via two 32-bit writes.
- `Buffer.from(wasmModule.HEAPU8.subarray(...))` creates an owned Buffer copy, so returned compressed/decompressed buffers remain safe after WASM pointers are freed.
- If a WASM output buffer is freed before retry allocation, the pointer must be cleared before any allocation that can throw; otherwise `finally` can double-free the old pointer on allocation failure.
- Python LZ4 matrix validation requires the `lz4==4.4.5` dependency to be present, as documented in `python/requirements.txt` and `python/README.md`; failures without that dependency are environment setup failures, not reader compatibility failures.

## Followup

No SOW-0021 follow-up is required.

## Regression Log

None.
