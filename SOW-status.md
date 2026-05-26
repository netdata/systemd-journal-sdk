# SOW Status

## Current

- None.

## Pending

- `SOW-0029-20260526-compression-threshold-parity.md` - open, needs user decision before implementation. Split from SOW-0022 Gap 5. Decide and implement cross-language compression threshold default/minimum behavior; current SDK defaults are 64 bytes while systemd defaults to 512 with an 8-byte minimum.
- `SOW-0030-20260526-monotonic-writer-validity.md` - open, needs user decision before implementation. Split from SOW-0022 Gap 8. Resolve low-level reject versus high-level clamp behavior for same-boot backward monotonic timestamps, then enforce the same policy across all four languages.
- `SOW-0031-20260526-compressed-compact-structural-parity.md` - open. Split from SOW-0022 Gap 7. Add structural parity validation for compressed and compact writer outputs, with byte identity only where deterministic and meaningful.
- `SOW-0032-20260526-live-feature-compatibility-matrix.md` - open. Split from SOW-0022 Gap 1. Extend live concurrency validation to compression, compact layout, compact plus compression, and FSS/sealed writer slices with stock journalctl, stock libsystemd, and repository readers.
- `SOW-0033-20260526-full-verification-parity.md` - open. Split from SOW-0022 Gap 2. Add practical systemd-like object-graph verification corruption fixtures and make all four language verification APIs reject the same classes as stock `journalctl --verify --file`.
- `SOW-0034-20260526-file-backed-journalctl-query-parity.md` - open. Split from SOW-0022 Gap 6. Complete remaining file-backed journalctl query/follow behavior: `--follow`, `--boot`, `--since`, and `--until`; daemon-only operations remain unsupported.
- `SOW-0009-20260523-benchmark-profile-optimize.md` - open, critical, sequenced after remaining compatibility feature/gap SOWs and before Netdata integration. Benchmark, profile, and optimize writers/readers before Netdata production replacement. Updated on 2026-05-26 after the user reported the Go SDK writer at about 5k logs/s in the SNMP traps ingestion worker versus about 25k logs/s for Netdata NetFlow with the vendored Rust implementation; later clarified that actual Netdata integration must happen last because the SDK does not yet perform well enough to replace the older vendored libraries.
- `SOW-0026-20260526-netdata-sdk-integration.md` - open, last pending integration SOW. Integrate the SDK into Netdata NetFlow reader/writer paths, OTEL writer path, OTEL signal viewer reader path, and no-libsystemd systemd-journal reader mode. Blocked until SOW-0009 shows acceptable performance or the user explicitly accepts a staged exception, and until the user authorizes the Netdata repository work target and dependency strategy. Netdata writers must default to compact format after integration.

## Done

- `SOW-0001-20260523-project-bootstrap-and-orchestration.md`
- `SOW-0002-20260523-repo-scaffold-and-rust-source-import.md`
- `SOW-0003-20260523-systemd-test-inventory-and-shared-harness.md`
- `SOW-0004-20260523-rust-sdk-and-journalctl.md`
- `SOW-0005-20260523-go-sdk-and-journalctl.md`
- `SOW-0006-20260523-node-sdk-and-journalctl.md`
- `SOW-0007-20260523-python-sdk-and-journalctl.md`
- `SOW-0008-20260523-interoperability-and-full-writer-features.md`
- `SOW-0010-20260523-go-reader-and-journalctl-completion.md`
- `SOW-0011-20260523-live-concurrency-compatibility-gate.md`
- `SOW-0012-20260523-go-writer-binary-fields.md`
- `SOW-0013-20260523-go-directory-writer-rotation-retention.md`
- `SOW-0014-20260524-deterministic-ingestion-dataset.md`
- `SOW-0015-20260524-deterministic-ingesters.md`
- `SOW-0016-20260524-byte-identical-writer-compatibility.md`
- `SOW-0017-20260524-xz-lz4-data-writing.md`
- `SOW-0018-20260524-compact-journal-format.md`
- `SOW-0019-20260524-forward-secure-sealing.md`
- `SOW-0020-20260524-directory-traversal-parity.md`
- `SOW-0021-20260524-nodejs-xz-data-compression.md`
- `SOW-0022-20260525-compatibility-test-gap-audit.md`
- `SOW-0023-20260525-netdata-ingestion-writer-api.md`
- `SOW-0024-20260526-mixed-format-directory-readers.md`
- `SOW-0025-20260526-retention-enforcement-on-open.md`
- `SOW-0027-20260526-netdata-reader-api-and-jf-facade.md`
- `SOW-0028-20260526-historical-header-parsing-parity.md`

## Notes

- The deterministic dataset must separate accepted rows from expected rejection cases.
- SOW-0015 produced deterministic ingesters for systemd C, Rust, Go, Node.js, and Python.
- SOW-0016 consumed the deterministic ingester outputs and is completed.
- SOW-0016 validation shows byte-for-byte identity for the accepted uncompressed corpus across online, offline, and archived final states, including exact DATA hash-chain depth parity, plus passing closed-file, binary, live, and zstd compression interoperability matrices.
- SOW-0017 completed xz/lz4 DATA writing for Rust/Go/Python and lz4 DATA writing for Node.js, with Node.js xz split to SOW-0021.
- SOW-0021 completed Node.js xz DATA reader/writer support through `node-liblzma@5.0.1` using the WASM-only runtime path accepted by user decision option B.
- SOW-0018 completed compact journal support. Every writer exposes explicit regular/compact output selection while regular remains the default. `run_compact_matrix.py` passes 56/56 for each compression mode (`none`, `zstd`, `xz`, `lz4`) across Go, Rust, Node.js, Python, stock journalctl, and stock libsystemd on systemd 260.1-2-manjaro.
- SOW-0019 Phase 2A added pure FSPRG primitives and vector tests in Rust, Go, Node.js, and Python. The primitives match committed systemd v260.1 vectors.
- SOW-0019 Phase 2B added unsealed journal verification APIs (`VerifyFile`, `verify_file`, etc.) in all four languages with controlled error types (`VerificationError`). The conformance case `journal-verify-corruption-detection` now produces real PASS/FAIL behavior instead of adapter skips.
- SOW-0019 Phase 3 added file-backed journalctl `--verify`, existing `--verify-only`, and `--verify-key` behavior in Rust, Go, Node.js, and Python. The rewrites parse `--verify-key` before verification, match stock invalid-key behavior on repo-local files, verify unsealed files through Phase 2B APIs, and follow symlinks to regular journal files during directory verification.
- SOW-0019 Phase 4 added sealed journal writers in Rust, Go, Node.js, and Python with deterministic test keys and configurable sealing intervals. Stock `journalctl --verify --verify-key` validates generated sealed files.
- SOW-0019 Phase 5 added sealed TAG/HMAC verification APIs and file-backed journalctl `--verify-key` validation in Rust, Go, Node.js, and Python. The shared `journal-verify-sealed` adapter case now runs real behavior in every language.
- SOW-0009 was originally sequenced last. The 2026-05-26 SNMP traps performance report makes it a critical Netdata integration gate. User scheduling decision on 2026-05-26: finish remaining compatibility feature/gap SOWs first, then run SOW-0009, then do SOW-0026 last because the SDK does not yet perform well enough to replace the faster vendored libraries.
- SOW-0025 completed open-time retention enforcement for Rust, Go, Node.js, and Python high-level directory writers. Eager/existing-active open enforces during construction; lazy archived-only construction remains side-effect-free until first append opens the active file, then retention runs before the first entry is written.
- SOW-0027 completed the accepted file-backed Netdata `jf`/libsystemd-like reader facade across Rust, Go, Node.js, and Python, including open file/directory/files, close, seek head/tail/realtime/cursor, next/previous/skip, match groups, current-entry data enumeration, field enumeration, unique enumeration, realtime/monotonic/seqnum/cursor metadata, boot listing, and binary/repeated value support. Netdata integration remains tracked by SOW-0026; broader compatibility gap validation is now split across SOW-0028 through SOW-0034.
- SOW-0020 completed directory traversal parity for SDK readers and file-backed `journalctl --directory` across Rust, Go, Node.js, and Python. `run_directory_matrix.py` passes against stock `journalctl` from systemd 260.1 and all repository rewrites for root files, one machine-id subdirectory level, interleaved ordering, matching, fields, boots, corrupt-skip, verify-skip, empty directories, and the repository `.journal.zst` directory extension.
- SOW-0024 completed mixed-format directory reader validation across stock journalctl plus Rust, Go, Node.js, and Python file-backed rewrites. `run_mixed_directory_matrix.py` passes 72/72 for mixed regular/compact files, uncompressed and zstd/xz/lz4 DATA-compressed files, sealed/unsealed files, active/archive names, directory verification key behavior, and repository whole-file `.journal.zst` / `.journal~.zst` extension discovery. No reader implementation changes were required.
- SOW-0022 was completed as a compatibility planning/triage SOW on 2026-05-26. Its stale gaps were closed by SOW-0019, SOW-0020, and SOW-0024 where applicable; the remaining executable work was split into SOW-0028 through SOW-0034.
- SOW-0028 completed historical header parsing parity. Rust, Go, Node.js, and Python now expose historical extension fields according to each field's on-disk `header_size` containment boundary, with added intermediate/future/truncated-prefix validation and matching Rust coverage in both `journal-core` and `jf/journal_file`.
- Byte-for-byte writer identity is the target for deterministic uncompressed journals. Any feature slice that cannot be made byte-identical must return with evidence before the acceptance condition is changed.
- The external systemd source checkout is read-only for this project. Build outputs and generated files must remain inside this repository or `/tmp`.
