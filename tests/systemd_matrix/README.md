# systemd Version Matrix

This framework builds selected systemd versions under `.local/systemd-matrix/`,
generates deterministic journal files with a systemd internal test helper, and
runs a sanitized reader matrix against version-built `journalctl`, stock
`journalctl`, and the Rust and Go SDK readers.

It must not be used against the live host journal. Inputs are explicit generated
files or caller-provided journal files, and stock systemd validation always uses
`journalctl --file`.

## Artifact Contract

- Build artifacts: `.local/systemd-matrix/builds/<version>/`
- Generated corpus files: `.local/systemd-matrix/corpus/<version>/`
- Generated FSS state and verification-key files:
  `.local/systemd-matrix/fss/` and `.local/systemd-matrix/secrets/`
- SDK helper builds and caches: `.local/systemd-matrix/sdk-build/`
- JSON and Markdown reports: `.local/systemd-matrix/reports/`

The `generate` and `test` commands require `--journal` paths to stay under
`.local/systemd-matrix/`. This prevents accidental reads from live host journal
locations and prevents generated output cleanup from touching user files.

Reports contain only sanitized metadata: status, counts, digests, byte sizes,
tool versions, command hashes, stderr/stdout hashes, and discrepancy codes.
They must not contain raw field names, field values, messages, hostnames, IPs,
usernames, binary payloads, or raw FSS verification keys.

## Commands

Build one systemd version:

```bash
python tests/systemd_matrix/run_systemd_matrix.py build --version v260.1 --systemd-src "$HOME/src/systemd.git"
```

Generate one deterministic corpus file:

```bash
python tests/systemd_matrix/run_systemd_matrix.py generate --version v260.1 --case smoke
```

Run the reader matrix for one generated file:

```bash
python tests/systemd_matrix/run_systemd_matrix.py test --version v260.1 --case smoke
```

Run the reader matrix for a prebuilt historical file and prebuilt historical
`journalctl` under `.local/systemd-matrix/`:

```bash
python tests/systemd_matrix/run_systemd_matrix.py test \
  --version v239 \
  --case historical-unkeyed-lz4-offline \
  --journal .local/systemd-matrix/versions/old-enterprise/corpus/v239/v239-compressed-offline.journal \
  --version-journalctl .local/systemd-matrix/versions/old-enterprise/build/v239/journalctl
```

Run the initial smoke end to end:

```bash
python tests/systemd_matrix/run_systemd_matrix.py smoke --version v260.1 --case smoke --systemd-src "$HOME/src/systemd.git"
```

Run a sealed/FSS smoke file without touching `/var/log/journal`:

```bash
python tests/systemd_matrix/run_systemd_matrix.py smoke \
  --version v260.1 \
  --case sealed-smoke \
  --sealed \
  --systemd-src "$HOME/src/systemd.git"
```

For sealed files, the runner patches only the `.local/systemd-matrix/builds/...`
copy of systemd so `journal_file_fss_load()` honors
`SYSTEMD_JOURNAL_FSS_ROOT`. Raw verification keys are written under
`.local/systemd-matrix/secrets/`; committed reports record only key hashes.
The generated sealed files use the systemd helper's runtime machine ID because
systemd FSS key lookup and the journal header machine ID must agree. These
files are therefore not byte-identical across hosts; matrix pass/fail decisions
use logical digests and stock `journalctl --verify --verify-key`.

Write a Markdown summary from any JSON report:

```bash
python tests/systemd_matrix/run_systemd_matrix.py summarize \
  --report .local/systemd-matrix/reports/matrix-v260.1-smoke.json \
  --markdown .local/systemd-matrix/reports/matrix-v260.1-smoke.summary.md
```

## Report Schema

All JSON reports use:

```text
schema: systemd-journal-sdk-systemd-matrix-v1
```

Reader matrix reports have:

- `kind`: `reader-matrix`
- `version`: requested systemd version label
- `source_commit`: checked systemd commit
- `case`: corpus case label
- `journal`: sanitized artifact path, byte size, and byte digest
- `tools`: version-built journalctl, stock journalctl, and SDK helper build metadata
- `results`: verify/read rows with status, counts, logical digest, and command hashes
- `baseline`: selected reader baseline, preferring modern stock journalctl
- `discrepancies`: structured discrepancy codes
- `observations`: non-blocking historical export differences
- `status`: `ok` or `failed`

## Discrepancy And Observation Codes

- `BUILD_FAILED`: systemd or SDK helper build failed
- `GENERATE_FAILED`: systemd helper could not generate the journal corpus
- `MISSING_TOOL`: a required local tool was unavailable
- `VERSION_VERIFY_FAILED`: version-built journalctl verification failed
- `STOCK_VERIFY_FAILED`: stock journalctl verification failed
- `VERSION_READ_FAILED`: version-built journalctl export read failed
- `STOCK_READ_FAILED`: stock journalctl export read failed
- `RUST_READ_FAILED`: Rust SDK digest helper failed
- `GO_READ_FAILED`: Go SDK digest helper failed
- `DIGEST_MISMATCH`: reader logical digest differs from the selected baseline
- `COUNT_MISMATCH`: reader logical counts differ from the selected baseline
- `VERSION_EXPORT_METADATA_DRIFT`: version-built `journalctl -o export`
  differs from modern stock output while counts match; modern stock/Rust/Go
  parity remains the pass/fail gate
- `VERSION_JOURNALCTL_UNAVAILABLE`: version build did not produce journalctl
- `VERIFY_KEY_MISSING`: sealed journal verification was requested but the
  runner could not find the matching verification key under `.local`
