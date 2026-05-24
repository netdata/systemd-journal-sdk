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

### Current Repository Support Matrix

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

- Pending implementation. Phase 1 records the systemd v260.1 compression frame requirements and current repository support gaps.

Tests or equivalent validation:

- Pending implementation. Phase 1 SOW-only validation requires `git diff --check` and `bash .agents/sow/audit.sh`.

Real-use evidence:

- Pending implementation. No writer/reader behavior has changed in Phase 1.

Reviewer findings:

- `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE`; no blocking findings. Non-blocking recommendation: present a Python standard-library native-module policy decision in Phase 2 because Python already uses standard-library `compression.zstd` while SOW-0017 flags standard-library `lzma`.
- `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE`; no blocking findings. Non-blocking recommendations were to cross-reference SOW-0016's compressed byte-identity carve-out, record exact Go dependency versions during Phase 2 verification, and make Phase 2 search for vendorable pure Node.js/Python LZ4 implementations before writing raw-block encoders from scratch.
- Both reviewer runs validated `git diff --check` and `bash .agents/sow/audit.sh`.
- Phase 1 implementer inventory was rejected as authoritative because local validation failed and primary sources contradicted several claims.

Same-failure scan:

- Pending implementation. Phase 2 must search for all zstd-only compression dispatches before editing.

Sensitive data gate:

- Phase 1 durable evidence uses public package names, public upstream paths, source line references, and synthetic fixture plans only. No raw secrets, credentials, bearer tokens, SNMP communities, community member names, customer names, personal data, non-private customer-identifying IPs, private endpoints, or proprietary incident details are recorded.

Artifact maintenance gate:

- AGENTS.md: no update for Phase 1; repository workflow rules did not change.
- Runtime project skills: updated `.agents/skills/project-agent-orchestration/SKILL.md` with package-manager cache redirection rules after reviewer package metadata checks exposed a repository-boundary failure mode.
- Specs: pending implementation; support matrices must be updated only after code behavior changes.
- End-user/operator docs: pending implementation; README support matrices must be updated only after code behavior changes.
- End-user/operator skills: no output/reference skills are affected by Phase 1.
- SOW lifecycle: active in `.agents/sow/current/` with `Status: in-progress`; completion still requires implementation, review, validation, status update to `completed`, move to `.agents/sow/done/`, audit, and one implementation-close commit.
- SOW-status.md: already updated on activation; no Phase 1 state change required.

Specs update:

- Pending implementation. No shipped behavior changed in Phase 1.

Project skills update:

- Updated `.agents/skills/project-agent-orchestration/SKILL.md` so future dependency-research prompts either forbid dependency-fetching commands or require package-manager caches under `.local/` or `/tmp`.

End-user/operator docs update:

- Pending implementation. No shipped behavior changed in Phase 1.

End-user/operator skills update:

- No output/reference skills are affected by Phase 1.

Lessons:

- Systemd XZ DATA objects must be treated as `.xz` streams with LZMA2 and `CHECK_NONE`; describing them as raw LZMA2 blocks is wrong and can lead agents to choose incompatible libraries.
- Dependency inventories must be validated against primary source evidence before they become implementation instructions.
- External-review prompts must forbid dependency-fetching commands unless every package-manager cache is explicitly redirected under `.local/` or `/tmp`; otherwise read-only package metadata checks can still create files outside the repository.

Follow-up mapping:

- Phase 2 implementation prompt must include exact XZ and LZ4 frame requirements from this SOW.
- Phase 2 implementation prompt must require exact Go module versions after fixture proof, not before.
- Phase 2 implementation prompt must require a search for vendorable pure Node.js/Python LZ4 block implementations before adding in-repo encoders from scratch.
- Any unresolved Node.js/Python pure-dependency gap must become either implemented in this SOW, rejected with evidence, or tracked by a real follow-up SOW before SOW-0017 can close.

## Outcome

Pending.

## Lessons Extracted

Pending implementation closeout.

## Followup

None yet.

## Regression Log

None yet.
