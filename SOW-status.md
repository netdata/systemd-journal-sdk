# SOW Status

## Current

- `SOW-0008-20260523-interoperability-and-full-writer-features.md` - in-progress. Cross-language interoperability is active; zstd writing and SDK writer-lock parity slices have passed local validation.

## Pending

- `SOW-0009-20260523-benchmark-profile-optimize.md` - open. Benchmark, profile, and optimize after correctness evidence is complete.
- `SOW-0014-20260524-deterministic-ingestion-dataset.md` - open. Define deterministic accepted, rejected, and performance ingestion corpora.
- `SOW-0015-20260524-deterministic-ingesters.md` - open. Build systemd C and SDK ingesters for the frozen dataset.
- `SOW-0016-20260524-byte-identical-writer-compatibility.md` - open. Require byte-for-byte deterministic writer compatibility against systemd for the accepted corpus.

## Done

- `SOW-0001-20260523-project-bootstrap-and-orchestration.md`
- `SOW-0002-20260523-repo-scaffold-and-rust-source-import.md`
- `SOW-0003-20260523-systemd-test-inventory-and-shared-harness.md`
- `SOW-0004-20260523-rust-sdk-and-journalctl.md`
- `SOW-0005-20260523-go-sdk-and-journalctl.md`
- `SOW-0006-20260523-node-sdk-and-journalctl.md`
- `SOW-0007-20260523-python-sdk-and-journalctl.md`
- `SOW-0010-20260523-go-reader-and-journalctl-completion.md`
- `SOW-0011-20260523-live-concurrency-compatibility-gate.md`
- `SOW-0012-20260523-go-writer-binary-fields.md`
- `SOW-0013-20260523-go-directory-writer-rotation-retention.md`

## Notes

- The deterministic dataset must separate accepted rows from expected rejection cases.
- Byte-for-byte writer identity is the target for deterministic uncompressed journals. Any feature slice that cannot be made byte-identical must return with evidence before the acceptance condition is changed.
- The external systemd source checkout is read-only for this project. Build outputs and generated files must remain inside this repository or `/tmp`.
