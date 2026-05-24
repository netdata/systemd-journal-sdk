# SOW-0017 - XZ And LZ4 DATA Writing

## Status

Status: in-progress

Sub-state: active after SOW-0016 byte-identical regular writer closeout.

## Requirements

### Purpose

Complete the remaining systemd-defined DATA-object compression writer formats beyond zstd, while preserving pure-language SDK guarantees and stock reader compatibility.

### User Request

The user requires journal writers to be compatible with stock journalctl and libsystemd readers. SOW-0008 delivered zstd DATA writing; xz and lz4 DATA writing remain open writer feature gaps.

### Assistant Understanding

Facts:

- zstd-compressed DATA object writing is implemented and validated across Rust, Go, Node.js, and Python.
- xz and lz4-compressed DATA object writing is not implemented.
- Rust reader support already handles xz/lz4 DATA objects through pure Rust dependencies; Go, Node.js, and Python current reader slices reject xz/lz4 DATA objects.
- Pure-language dependencies are allowed after dependency review. CGO, native Node.js addons, and system journal libraries remain forbidden.

Inferences:

- Reader support for xz/lz4 may need to be implemented before writer parity can be claimed for all languages.
- Dependency availability may differ by language and compression family, so this SOW may split xz and lz4 into separate implementation chunks if one format is ready before the other.

Unknowns:

- Which pure-language xz/lz4 dependencies are acceptable for Go, Node.js, and Python after license, maintenance, performance, and compatibility review.
- Whether stock systemd v260.1 accepts all chosen pure-library frame outputs without additional frame metadata normalization.

### Acceptance Criteria

- A dependency review records pure-language xz and lz4 options for Rust, Go, Node.js, and Python.
- Readers in all four languages either support xz/lz4-compressed DATA objects or the SOW stops with evidence and a user decision before writer claims are weakened.
- Writers in all four languages can write xz and lz4-compressed DATA objects when configured, or the SOW is split by compression family with evidence.
- A shared compression matrix proves header/object flags, stock `journalctl --verify --file`, stock journalctl reads, stock libsystemd reads, and all repository readers for every implemented compression family.
- Uncompressed and zstd writing remain compatible and unchanged.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/current/SOW-0008-20260523-interoperability-and-full-writer-features.md`
- `.agents/sow/specs/product-scope.md`
- `tests/interoperability/run_compression_matrix.py`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
- `src/libsystemd/sd-journal/journal-def.h`
- `src/libsystemd/sd-journal/journal-file.c`

Current state:

- zstd DATA writing passes the compression interoperability matrix.
- xz/lz4 writing remains unimplemented; Go, Node.js, and Python readers currently reject xz/lz4 DATA objects.

Risks:

- Compression frame details can be library-specific while still semantically valid.
- New compression dependencies can add maintenance or security risk.
- Performance can regress if compression objects are allocated per DATA value without pooling.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The final writer target includes compression where systemd journal files define it. zstd is complete, but xz and lz4 are still missing, leaving the compression writer target incomplete.

Evidence reviewed:

- SOW-0008 records xz/lz4 DATA writing as an explicit remaining writer gap.
- Product scope lists xz/lz4 DATA object writing as unimplemented for current SDK slices.
- systemd journal object flags define xz, lz4, and zstd DATA compression families.

Affected contracts and surfaces:

- Writer compression options.
- Reader DATA decompression behavior.
- File-backed journalctl JSON/export/text output.
- Compression interoperability matrix.
- Dependency policy and documentation.

Existing patterns to reuse:

- zstd writer options and threshold behavior from SOW-0008.
- `tests/interoperability/run_compression_matrix.py`.
- Per-language livewriter compression fixture modes.
- Stock journalctl and libsystemd validation helpers.

Risk and blast radius:

- Medium to high. Compression touches writer object storage, reader parsing, matching, and dependency surfaces across all languages.

Sensitive data handling plan:

- Use synthetic compression fixtures only. Durable artifacts record commands, verdicts, dependency names, licenses, and sanitized diagnostics; no secrets or customer data.

Implementation plan:

1. Inventory systemd xz/lz4 frame requirements and pure-language dependency candidates per language.
2. Add reader support where missing before writer compatibility is claimed.
3. Add writer options for xz/lz4 using the existing compression-option pattern.
4. Extend the compression matrix by compression family.
5. Run full regression matrices and dependency review.

Validation plan:

- Extended compression matrix for every implemented family.
- Existing zstd, binary, live, and closed-file matrices remain passing.
- Language package tests remain passing.
- Dependency audit records pure-language status and licenses.
- External reviewers confirm no native linkage or compatibility weakening.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: update if compression workflow changes durable future validation.
- Specs: update product scope with exact reader/writer support per language.
- End-user/operator docs: update README support matrices.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: remains pending until activated; may split xz/lz4 if dependency evidence requires.
- SOW-status.md: update when activated or closed.

Open-source reference evidence:

- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
- `src/libsystemd/sd-journal/journal-def.h`
- `src/libsystemd/sd-journal/journal-file.c`

Open decisions:

- None blocking activation. If pure-language dependency review fails for a language/compression family, stop and present evidence before changing scope.

## Implications And Decisions

1. Compression-family boundary
   - Decision: track xz and lz4 in one SOW initially, but allow splitting by compression family after dependency evidence.
   - Reason: both families share the same journal object/header mechanics, but dependency feasibility may differ.
   - Risk: forcing both families into one implementation chunk could delay a production-ready subset.

## Plan

1. Review systemd xz/lz4 implementation and pure dependency options.
2. Implement missing reader support.
3. Implement writer support by compression family.
4. Extend shared matrix and docs.
5. Review and commit verified chunks.

## Delegation Plan

Implementer:

- Preferred implementer is `llm-netdata-cloud/minimax-m2.7-coder`.

Reviewers:

- At least two reviewers from the approved pool.

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

- Activated after SOW-0016 completion and after sequencing SOW-0009 behind remaining feature-completeness SOWs.
- Ran a Phase 1 inventory implementer pass with `llm-netdata-cloud/minimax-m2.7-coder`. The pass edited only this SOW, but its results were not accepted as authoritative because `git diff --check` failed, the SOW audit failed after required validation sections were removed, and several compression/dependency claims were contradicted by primary source evidence.
- Repaired the Phase 1 inventory against systemd v260.1 source and package documentation before using it to steer implementation.
- Ran read-only reviews with `llm-netdata-cloud/qwen3.6-plus` and `llm-netdata-cloud/kimi-k2.6`; both returned `PRODUCTION GRADE` for the repaired Phase 1 inventory.
- Reviewer orchestration note: reviewers performed package-fetch metadata checks that may have written tool caches outside this repository despite prompt instructions. Future prompts must explicitly forbid dependency-fetching commands unless `GOMODCACHE`, `GOCACHE`, `GOPATH`, `npm_config_cache`, `PIP_CACHE_DIR`, and equivalent caches are forced under `.local/` or `/tmp`.
- Started a Phase 2A implementation pass with `llm-netdata-cloud/minimax-m2.7-coder`, then stopped it before code edits because it ran package/doc/source inspection commands against default cache paths despite explicit cache redirection rules. `git status --short` showed no tracked changes after the stop, but ignored `rust/target/` output was generated inside this repository.

### 2026-05-24 - Phase 2A Rust + Go Implementation (fallback: llm-netdata-cloud/qwen3.6-plus)

- Implemented as fallback implementer per AGENTS.md model hierarchy after Phase 2A minimax run was stopped.
- Scope: Rust and Go xz/lz4 DATA object compression writer support, reader support, shared compression matrix updates, and validation.
- The qwen implementer run hit its 30-minute timeout while updating this SOW. The project manager inspected the diff, fixed integration issues, formatted the touched code, and reran validation before review.

Files changed by this Phase 2A chunk:

- `rust/Cargo.toml`: added the `encoder` feature to the existing `lzma-rust2` workspace dependency.
- `rust/src/crates/journal-core/src/file/file.rs`: added `Compression::Xz` and `Compression::Lz4`, mapped their header incompatible flags, and preserved compression choice when creating successor files.
- `rust/src/crates/journal-core/src/file/writer.rs`: preserved xz/lz4 compression when reopening an existing compressed file, added xz/lz4 writer compression paths, and added systemd-compatible XZ/LZ4 DATA payload helpers.
- `rust/src/internal/testcmd/livewriter/src/main.rs`: added `--xz-fixture` and `--lz4-fixture` fixture modes and compression parsing.
- `go/go.mod` and `go/go.sum`: added pure-Go `github.com/pierrec/lz4/v4 v4.1.26` and `github.com/ulikunitz/xz v0.5.15`.
- `go/journal/format.go`: added xz/lz4 compression constants and a 768 MiB decompressed DATA-object limit matching the Rust reader limit.
- `go/journal/reader.go`: added xz/lz4 DATA decompression and reader incompatible-flag support, with LZ4 size-prefix validation and xz decompression limiting.
- `go/journal/writer.go`: added xz/lz4 DATA compression, writer reopen support for xz/lz4 files, invalid compression rejection, and decompression helpers for duplicate-data lookup.
- `go/journal/writer_test.go`: added a focused zstd/xz/lz4 create, reopen, compressed-object flag, and reader round-trip test.
- `go/internal/testcmd/livewriter/main.go`: added xz/lz4 fixture flags and rejects unknown compression strings.
- `tests/interoperability/run_compression_matrix.py`: parameterized the compression family, added xz/lz4 header/object flag checks, kept default execution at zstd for all-language compatibility, and redirected Go/Cargo build caches under root `.local/`.
- `tests/interoperability/README.md`: documented the compression-family matrix behavior and current xz/lz4 partial support.
- `.agents/sow/specs/product-scope.md`: updated Rust and Go reader/writer support slices for xz/lz4.
- `.agents/sow/current/SOW-0017-20260524-xz-lz4-data-writing.md`: recorded execution, validation, and remaining scope.

Validation commands and results run after project-manager fixes:

- `GOMODCACHE=$PWD/../.local/go/pkg/mod GOCACHE=$PWD/../.local/go-build GOPATH=$PWD/../.local/go go test ./...` from `go/` - passed.
- `CARGO_HOME=$PWD/.local/cargo-home CARGO_TARGET_DIR=$PWD/.local/cargo-target cargo test --manifest-path rust/Cargo.toml` - passed.
- `python3 -m py_compile tests/interoperability/run_compression_matrix.py` - passed.
- `python3 tests/interoperability/run_compression_matrix.py --writers rust go --readers rust go stock --compression xz lz4 --entries 5 --keep-files` - 48/48 passed.
- `python3 tests/interoperability/run_compression_matrix.py --writers rust go --readers rust go stock --compression zstd --entries 5 --keep-files` - 24/24 passed.
- `python3 tests/interoperability/run_compression_matrix.py --compression zstd --entries 5 --keep-files` - 72/72 passed across Go, Rust, Node.js, Python writers and readers.
- `python3 tests/interoperability/run_compression_matrix.py --entries 5 --keep-files` - 72/72 passed, proving default behavior remains zstd-only for all-language compatibility.
- `git diff --check` - passed.
- `bash .agents/sow/audit.sh` - passed.

Dependency versions:
- Rust: `lz4_flex 0.12.2` (existing), `lzma-rust2 0.15.8` (existing, added `encoder` feature).
- Go: `github.com/pierrec/lz4/v4 v4.1.26` (new), `github.com/ulikunitz/xz v0.5.15` (new).

Remaining Node/Python decisions:
- Node.js xz/lz4 writing remains unimplemented; no pure-JS xz/LZMA2 encoder or raw-block LZ4 package was found that meets the pure-language policy. Node.js xz/lz4 writing requires either in-repo implementation or a user-approved scope split.
- Python xz writing: Python standard library `lzma` module can produce `.xz` streams with `CHECK_NONE` and `FILTER_LZMA2`, but policy acceptance must be handled consistently with existing Python zstd decision.
- Python lz4 writing: PyPI `lz4` has C bindings and is not acceptable under pure-language policy; requires in-repo pure Python raw-block implementation.
- Node.js and Python xz/lz4 reader support also remains unimplemented and should be addressed in a follow-up SOW or this SOW's remaining scope.

## Phase 1 - Dependency And systemd Reference Inventory

Date: 2026-05-24

Systemd reference:

- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
- Tag: `v260.1`

### systemd v260.1 DATA Compression Format

Evidence:

- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
- `src/libsystemd/sd-journal/journal-def.h:45-50`
- `src/libsystemd/sd-journal/journal-def.h:166-171`
- `src/basic/compress.c:170-214`
- `src/basic/compress.c:250-279`
- `src/basic/compress.c:347-445`
- `src/libsystemd/sd-journal/journal-file.c:1808-1842`
- `src/libsystemd/sd-journal/journal-file.c:1884-1894`

Confirmed facts:

- DATA object compression flags are `OBJECT_COMPRESSED_XZ = 1 << 0`, `OBJECT_COMPRESSED_LZ4 = 1 << 1`, and `OBJECT_COMPRESSED_ZSTD = 1 << 2`.
- Header incompatible flags are `HEADER_INCOMPATIBLE_COMPRESSED_XZ = 1 << 0`, `HEADER_INCOMPATIBLE_COMPRESSED_LZ4 = 1 << 1`, `HEADER_INCOMPATIBLE_KEYED_HASH = 1 << 2`, and `HEADER_INCOMPATIBLE_COMPRESSED_ZSTD = 1 << 3`.
- XZ compression uses `lzma_stream_buffer_encode()` with an LZMA2 filter chain and `LZMA_CHECK_NONE`; the on-disk DATA payload is an `.xz` stream with no journal-specific size prefix.
- LZ4 compression writes an 8-byte little-endian uncompressed size prefix followed by a raw LZ4 block produced by `LZ4_compress_default()` or `LZ4_compress_HC()`.
- LZ4 decompression reads the 8-byte prefix and then calls `LZ4_decompress_safe(src + 8, out, src_size - 8, size)`.
- systemd attempts compression into a destination budget of `payload_size - 1`; if compression fails or does not fit, the DATA object is stored uncompressed.
- systemd has hard compression-family minimums before attempting compression: XZ returns `-ENOBUFS` for payloads below 80 bytes, and LZ4 returns `-ENOBUFS` for payloads below 9 bytes.

Compatibility implications:

- XZ support must produce and read `.xz` streams using LZMA2 and no integrity check, not raw LZMA2 blocks and not the legacy `.lzma` format.
- LZ4 support must use raw block APIs and manually preserve systemd's 8-byte uncompressed-size prefix. LZ4 frame APIs and size-prepended helper formats are not directly compatible unless adapted.
- Tests must assert object flags, header incompatible flags, actual compressed DATA objects, stock `journalctl --verify --file`, stock journalctl output, stock libsystemd reads, and every repository reader.
- Byte-identical compressed output is not required by SOW-0016; valid stock-reader-compatible compressed DATA output is required unless a later SOW/user decision raises the requirement.

### Phase 1 Pre-Implementation Repository Support Matrix

This matrix records the starting point before Phase 2A edits. Current shipped support after Phase 2A is tracked in `.agents/sow/specs/product-scope.md`.

| Language | Read XZ DATA | Read LZ4 DATA | Write XZ DATA | Write LZ4 DATA | Evidence |
| --- | --- | --- | --- | --- | --- |
| Rust | Yes | Yes | No | No | `rust/src/crates/journal-core/src/file/object.rs:1097-1110`, `rust/src/crates/journal-core/src/file/writer.rs:401-413` |
| Go | No | No | No | No | `go/journal/writer.go:16`, `go/journal/writer.go:653-657`, `go/journal/writer.go:1088-1110` |
| Node.js | No | No | No | No | `node/src/lib/entry.js:65-74`, `node/src/lib/writer.js:299-308`, `node/src/lib/writer.js:740-743` |
| Python | No | No | No | No | `python/journal/entry.py:66-70`, `python/journal/writer.py:293-299`, `python/journal/writer.py:730-744` |

### Dependency Inventory

Rust:

- Existing `lz4_flex` dependency exposes raw block `compress`, `compress_into`, and `decompress_into` APIs. Systemd's 8-byte prefix still has to be written manually because `compress_prepend_size` uses its own little-endian size-prepended helper format.
- Existing `lzma-rust2` dependency includes `XzReader`, `XzWriter`, `XzOptions`, and encoder feature support. This matches the systemd XZ stream requirement better than raw LZMA2.
- No new Rust compression dependency is expected for Phase 2.

Go:

- Existing `github.com/klauspost/compress` is used only for zstd in this repository. No verified LZ4 or XZ DATA support exists in the current Go code.
- `github.com/pierrec/lz4/v4` is a strong LZ4 candidate because it exposes pure-Go raw block `Compressor.CompressBlock`, `CompressorHC.CompressBlock`, `UncompressBlock`, and `CompressBlockBound` APIs.
- `github.com/ulikunitz/xz` is a strong XZ candidate because it exposes a Go XZ writer and `WriterConfig` supports disabling the checksum. Its standard package writes `.xz` streams; the deeper `github.com/ulikunitz/xz/v2/lzma` package exposes raw LZMA2 APIs but is a v2 development line and should not be selected without stronger evidence.
- Go dependency choice requires implementer verification in Phase 2 with a small stock-systemd compatibility fixture before broad code changes; record exact proven module versions before broad edits.

Node.js:

- `node:zlib` currently provides zstd support only in this repository.
- `lz4js` is pure JavaScript and has no dependencies, but its public high-level API is framed LZ4 and its npm documentation states it does not support raw block data. It is not acceptable for systemd DATA LZ4 unless lower-level code can be safely reused or a raw block implementation is added in-repo.
- `lz4` has block APIs but includes native bindings, so it is not acceptable under the current no-native-addon policy unless a documented pure-JavaScript block path can be isolated and proven.
- `xz` is a liblzma binding and is not acceptable.
- `lzma-purejs` is pure JavaScript LZMA, but the npm documentation describes LZMA `.lzma` compression, not XZ/LZMA2 stream writing with `CHECK_NONE`.
- Node.js XZ writing remains high-risk until a pure JavaScript XZ/LZMA2 encoder or an in-repo implementation plan is proven.

Python:

- Python's standard `lzma` module can produce `.xz` container streams, supports `CHECK_NONE`, and supports `FILTER_LZMA2`. This matches the systemd XZ stream format semantically, but it is a CPython standard-library module backed by the runtime's lzma implementation, so policy acceptance must be handled consistently with the existing Python zstd decision.
- `python-xz` is a pure-Python XZ file-format layer, but its own description says it leverages the `lzma` module for compression, so it does not avoid the standard-library/native compression question.
- PyPI `lz4` supports LZ4 block compression and can disable its own stored-size prefix, but it is Python bindings for the LZ4 library and has C classifiers, so it is not acceptable under the current pure-language dependency policy.
- Python LZ4 likely requires an in-repo pure Python raw-block implementation if no acceptable pure dependency is found.

### Phase 1 Conclusion

Ready for implementation planning:

- Rust LZ4 and XZ can likely use existing dependencies.
- Go LZ4 and XZ have strong pure-Go candidates, but Phase 2 must prove exact stock-reader compatibility before broad edits.

Needs deeper dependency or implementation design before claims:

- Node.js LZ4 may need in-repo raw-block code instead of npm package use.
- Node.js XZ may need in-repo XZ/LZMA2 code, a carefully audited pure JS package that was not yet found, or a user-approved scope split.
- Python XZ needs a policy decision if standard-library native compression is considered acceptable for this project slice.
- Python LZ4 likely needs in-repo raw-block code instead of PyPI `lz4`.

No compatibility claims are weakened by this inventory. If a language/format cannot satisfy pure-language and stock-reader requirements in Phase 2, this SOW must stop and present the user with options before narrowing scope.

## Validation

Acceptance criteria evidence:

- Rust xz/lz4 writer support: implemented via `Compression::Xz` and `Compression::Lz4` enum variants, `stored_data_payload()` dispatch, and `xz_compress()`/`lz4_compress()` helpers.
- Go xz/lz4 reader and writer support: implemented via `CompressionXZ`/`CompressionLZ4` constants, `addData()` dispatch, `readDataPayload()` decompression, and helper functions.
- Shared compression matrix: extended `run_compression_matrix.py` to support xz and lz4 families; 48/48 tests pass across xz/lz4 for Rust and Go writers with stock journalctl, stock libsystemd, Rust, and Go readers.
- zstd writing remains compatible: zstd matrix tests pass 24/24 for Rust/Go scope and 72/72 for the default all-language scope.
- No changes outside this repository.

Tests or equivalent validation:

- Rust: `cargo test --manifest-path rust/Cargo.toml` - passed.
- Go: `go test ./...` from `go/` - passed.
- Python syntax: `python3 -m py_compile tests/interoperability/run_compression_matrix.py` - passed.
- Compression matrix (xz/lz4): 48/48 pass (Rust + Go writers, stock + libsystemd + Rust + Go readers).
- Compression matrix (zstd Rust/Go): 24/24 pass.
- Compression matrix default/all-language zstd: 72/72 pass.
- `git diff --check`: clean.
- `bash .agents/sow/audit.sh`: clean.

Real-use evidence:

- Stock `journalctl --verify --file` passes for all xz/lz4 journals written by Rust and Go.
- Stock `journalctl --output=json` reads back all xz/lz4 entries correctly.
- Stock `journalctl --output=export` reads back all xz/lz4 entries correctly.
- Stock libsystemd reader (`libsystemd_binary_field_reader`) reads xz/lz4 entries correctly.
- Rust repository reader reads Go xz/lz4 journals and vice versa.
- Go repository reader reads Rust xz/lz4 journals and vice versa.

Reviewer findings:

- Phase 2A `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`; no blocking findings. Non-blocking observations: Go LZ4 compression could defensively treat a theoretical `CompressBlock` zero-length output as uncompressed, the reopen compression threshold resets to the default because the threshold is not stored in the journal header, and Go zstd per-DATA decoder allocation is a pre-existing performance consideration.
- Phase 2A `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`; no blocking findings. Non-blocking observations: same theoretical Go LZ4 zero-length-output guard, pre-existing reopen threshold reset, and pre-existing per-object Go zstd decoder allocation.
- Disposition: added the Go LZ4 zero-length-output guard. Reopen threshold reset and per-object Go zstd decoder allocation are pre-existing behavior outside the xz/lz4 compatibility change; they remain non-blocking and can be revisited during the benchmark/profiling SOW.
- Phase 2A second-pass `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`; no blocking findings after the Go LZ4 guard. Non-blocking observations were pre-existing Go reopen threshold reset and pre-existing per-object Go zstd decoder allocation.
- Phase 2A second-pass `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`; no blocking findings after the Go LZ4 guard. Non-blocking observations were pre-existing Go reopen threshold reset, pre-existing per-object Go zstd decoder allocation, and matrix usability if a caller explicitly asks for xz/lz4 without restricting writers to Rust/Go.
- `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`; no blocking findings. Non-blocking recommendation: present a Python standard-library native-module policy decision in Phase 2 because Python already uses standard-library `compression.zstd` while SOW-0017 flags standard-library `lzma`.
- `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`; no blocking findings. Non-blocking recommendations were to cross-reference SOW-0016's compressed byte-identity carve-out, record exact Go dependency versions during Phase 2 verification, and make Phase 2 search for vendorable pure Node.js/Python LZ4 implementations before writing raw-block encoders from scratch.
- Phase 1 reviewer runs validated `git diff --check` and `bash .agents/sow/audit.sh`.
- Phase 1 implementer inventory was rejected as authoritative because local validation failed and primary sources contradicted several claims.

Same-failure scan:

- Searched all zstd-only compression dispatches before editing; updated all relevant paths in Rust (`writer.rs:stored_data_payload`, `file.rs:Compression`), Go (`writer.go:addData`, `reader.go:readDataPayload`, `format.go:Compression*`), and test harness (`run_compression_matrix.py`).

Sensitive data gate:

- Phase 2A durable evidence uses public package names, public upstream paths, source line references, and synthetic fixture plans only. No raw secrets, credentials, bearer tokens, SNMP communities, community member names, customer names, personal data, non-private customer-identifying IPs, private endpoints, or proprietary incident details are recorded.

Artifact maintenance gate:

- AGENTS.md: no update; repository workflow rules did not change.
- Runtime project skills: no update; compression workflow did not change durable future validation rules.
- Specs: updated `.agents/sow/specs/product-scope.md` for Rust/Go xz/lz4 support and left Node/Python limitations intact.
- End-user/operator docs: updated `tests/interoperability/README.md` for the parameterized compression matrix and current xz/lz4 partial support.
- End-user/operator skills: no output/reference skills are affected by Phase 2A.
- SOW lifecycle: active in `.agents/sow/current/` with `Status: in-progress`; Node/Python xz/lz4 scope remains open.
- SOW-status.md: no state change required until SOW close.

Specs update:

- Updated `.agents/sow/specs/product-scope.md` so the current Rust and Go reader/writer support slices include xz/lz4 DATA objects, while Node.js and Python still explicitly list xz/lz4 as unsupported.

Project skills update:

- No update required for Phase 2A.

End-user/operator docs update:

- Updated `tests/interoperability/README.md` to explain zstd default coverage, explicit `--compression xz lz4` usage, and the partial Rust/Go-only xz/lz4 support state.

End-user/operator skills update:

- No output/reference skills are affected by Phase 2A.

Lessons:

- Systemd XZ DATA objects must be treated as `.xz` streams with LZMA2 and `CHECK_NONE`; describing them as raw LZMA2 blocks is wrong and can lead agents to choose incompatible libraries.
- Dependency inventories must be validated against primary source evidence before they become implementation instructions.
- External-review prompts must forbid dependency-fetching commands unless every package-manager cache is explicitly redirected under `.local/` or `/tmp`; otherwise read-only package metadata checks can still create files outside the repository.
- Rust `lzma-rust2` requires the `encoder` feature flag to expose `XzWriter`/`XzOptions`; the default workspace dependency only had `xz` feature (reader-only).
- Go `github.com/pierrec/lz4/v4` `CompressBlock` requires a third `[]int` argument (ignored); Go `github.com/ulikunitz/xz` uses `WriterConfig{NoCheckSum: true}` pattern, not a constant.
- Go reader `supportedReaderIncompatible` constant must include xz/lz4 flags to avoid rejecting xz/lz4 journals on open.
- Go writer `supportedWriterIncompatible` must also include xz/lz4 flags, or `Open()` rejects files that `Create()` can produce.
- Rust reopen/successor paths must preserve xz/lz4 compression flags, not only zstd.
- Compression matrix test harness must set `CARGO_TARGET_DIR` itself before copying Rust binaries, otherwise it can copy from the wrong target directory depending on caller environment.

Follow-up mapping:

- Node.js xz/lz4 writing: requires either in-repo pure-JS implementation or user-approved scope split. No acceptable pure-JS xz/LZMA2 encoder or raw-block LZ4 package was found.
- Python xz writing: requires policy decision on standard-library `lzma` module acceptance (consistent with existing Python zstd decision).
- Python lz4 writing: requires in-repo pure Python raw-block implementation (PyPI `lz4` has C bindings).
- Node.js and Python xz/lz4 reader support: should be addressed in a follow-up SOW or this SOW's remaining scope.
- Node.js and Python xz/lz4 support remains active SOW scope; if it is split, create real follow-up SOW files before closing this SOW.

## Outcome

Pending.

## Lessons Extracted

Pending implementation closeout.

## Followup

None yet.

## Regression Log

None yet.
