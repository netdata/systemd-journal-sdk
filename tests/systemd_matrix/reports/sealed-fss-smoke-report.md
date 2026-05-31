# SOW-0064 Sealed FSS systemd Matrix Supplement

- Created: `2026-05-31T21:07:52Z`
- Purpose: close the previous `fss_not_generated` coverage gap without writing
  host journal state under `/var/log/journal`.
- Sensitive data policy: verification keys are stored only under `.local/`;
  durable reports record only verification-key SHA-256 digests and sanitized
  result metadata.
- Generated sealed files: `10`
- Passed sealed files: `10`
- Discrepancies: `0`

## Scope

The matrix runner patches only repository-local `.local/systemd-matrix/...`
systemd source copies so `journal_file_fss_load()` reads
`SYSTEMD_JOURNAL_FSS_ROOT` when set, falling back to systemd's stock
`/var/log/journal/<machine-id>/fss` path. The generated FSS key material and
verification keys remain under `.local/systemd-matrix/` and are not committed.

## Results

| version | source | format | status | discrepancies | observations | entries | payloads | binary payloads |
| --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: |
| `v252` | `non-git-source-sha256:e55d1c7ffd6b5c7828548eb4` | `regular` | `ok` | `0` | `1` | `349` | `3474` | `2` |
| `v252` | `non-git-source-sha256:e55d1c7ffd6b5c7828548eb4` | `compact` | `ok` | `0` | `1` | `349` | `3474` | `2` |
| `v254` | `non-git-source-sha256:cac760cd8fb0d211bae70d0e` | `regular` | `ok` | `0` | `0` | `349` | `3474` | `2` |
| `v254` | `non-git-source-sha256:cac760cd8fb0d211bae70d0e` | `compact` | `ok` | `0` | `0` | `349` | `3474` | `2` |
| `v258.8` | `8d9de518e84872e29a6339bbc56a51e0e471d930` | `regular` | `ok` | `0` | `0` | `349` | `3474` | `2` |
| `v258.8` | `8d9de518e84872e29a6339bbc56a51e0e471d930` | `compact` | `ok` | `0` | `0` | `349` | `3474` | `2` |
| `v260.1` | `c0a5a2516d28601fb3afc1a77d7b42fcfe38fced` | `regular` | `ok` | `0` | `0` | `349` | `3474` | `2` |
| `v260.1` | `c0a5a2516d28601fb3afc1a77d7b42fcfe38fced` | `compact` | `ok` | `0` | `0` | `349` | `3474` | `2` |
| `v260.2` | `f1d0952a125b96b7ab2f1ff29a87448ade8ac29b` | `regular` | `ok` | `0` | `0` | `349` | `3474` | `2` |
| `v260.2` | `f1d0952a125b96b7ab2f1ff29a87448ade8ac29b` | `compact` | `ok` | `0` | `0` | `349` | `3474` | `2` |

## Notes

- `v252` still records `VERSION_EXPORT_METADATA_DRIFT`: the version-built
  `journalctl -o export` omits metadata that current stock journalctl, Rust,
  and Go agree on. Counts match and this is not an SDK discrepancy.
- Every sealed file was verified by version-built `journalctl --verify --file
  --verify-key`, stock `journalctl --verify --file --verify-key`, Rust reader,
  and Go reader.
- The same deterministic correctness dataset was used for each case and includes
  binary fields, repeated field names, empty fields, large values, and mixed
  cardinality values.
