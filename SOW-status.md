# SOW Status

## Current

- `SOW-0017-20260524-xz-lz4-data-writing.md` - in-progress. Add xz/lz4 DATA compression reader/writer support after dependency review.

## Pending

- `SOW-0018-20260524-compact-journal-format.md` - open. Add compact journal format support after reference inventory.
- `SOW-0019-20260524-forward-secure-sealing.md` - open. Add FSS and full verification support without daemon-only lifecycle commands.
- `SOW-0020-20260524-directory-traversal-parity.md` - open. Bring SDK directory readers and file-backed journalctl `--directory` behavior to stock parity.
- `SOW-0009-20260523-benchmark-profile-optimize.md` - open. Final benchmark, profile, and optimize pass after SOW-0017 through SOW-0020 are complete.

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

## Notes

- The deterministic dataset must separate accepted rows from expected rejection cases.
- SOW-0015 produced deterministic ingesters for systemd C, Rust, Go, Node.js, and Python.
- SOW-0016 consumed the deterministic ingester outputs and is completed.
- SOW-0016 validation shows byte-for-byte identity for the accepted uncompressed corpus across online, offline, and archived final states, including exact DATA hash-chain depth parity, plus passing closed-file, binary, live, and zstd compression interoperability matrices.
- SOW-0009 is intentionally sequenced last. The user decided not to run baseline-only benchmarks now because performance work is expected to require profiling, allocation reduction, buffer reuse, and refactoring that later feature SOWs could invalidate.
- Byte-for-byte writer identity is the target for deterministic uncompressed journals. Any feature slice that cannot be made byte-identical must return with evidence before the acceptance condition is changed.
- The external systemd source checkout is read-only for this project. Build outputs and generated files must remain inside this repository or `/tmp`.
