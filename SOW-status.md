# SOW Status

## Current

- `SOW-0023-20260525-netdata-ingestion-writer-api.md` - in-progress. Implementing the cross-language Netdata ingestion writer API superset needed by NetFlow, OTEL logs, and SNMP traps integration. Current verified slice aligns high-level writer naming, chain resume identity, strict naming sequence continuity, committed-byte retention accounting, duration rotation, age retention, explicit retention enforcement, active/current retention protection, empty-active crash recovery, and rotation/archive retry cleanup across Rust, Go, Node.js, and Python. Active slice is Go API stabilization for external Netdata integration: eager open, strict identity, public path accessors, lifecycle callbacks, artifact-size retention accounting, source realtime injection, optional policy semantics, and a `go/v0.1.0` API contract.

## Pending

- `SOW-0022-20260525-compatibility-test-gap-audit.md` - open. Records compatibility test gaps and likely feature-parity gaps found during read-only review. User decisions recorded: full unsealed verification parity remains here later; directory traversal remains in SOW-0020; compressed/compact parity is structural unless byte identity is deterministic; invalid same-boot monotonic writer appends must be rejected; file-backed journalctl option parity remains in scope across follow-up work.
- `SOW-0020-20260524-directory-traversal-parity.md` - open. Bring SDK directory readers and file-backed journalctl `--directory` behavior to stock parity.
- `SOW-0009-20260523-benchmark-profile-optimize.md` - open. Final benchmark, profile, and optimize pass after SOW-0017 through SOW-0021 are complete.

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
- `SOW-0021-20260524-nodejs-xz-data-compression.md`

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
- SOW-0009 is intentionally sequenced last. The user decided not to run baseline-only benchmarks now because performance work is expected to require profiling, allocation reduction, buffer reuse, and refactoring that later feature SOWs could invalidate.
- Byte-for-byte writer identity is the target for deterministic uncompressed journals. Any feature slice that cannot be made byte-identical must return with evidence before the acceptance condition is changed.
- The external systemd source checkout is read-only for this project. Build outputs and generated files must remain inside this repository or `/tmp`.
