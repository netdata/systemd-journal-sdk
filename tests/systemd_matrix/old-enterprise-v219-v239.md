# Old Enterprise systemd Matrix: v219 and v239

SOW-0064 version-specific execution report for old enterprise systemd
versions. The shared `tests/systemd_matrix` framework was not present when this
work ran, so this file is a report only, not a runner or framework.

Sensitive-data gate: generated journals are synthetic. This durable report
stores only artifact identifiers, counts, digests, status codes, feature flags,
and discrepancy codes. It does not store raw journal payloads, field values,
hostnames, IPs, usernames, messages, or binary payloads.

## Scope

- `v219`: RHEL/CentOS 7 class.
- `v239`: RHEL 8 class.
- Source reference: `systemd/systemd`, inspected from the local read-only
  mirror and recorded by upstream commit in the table below.
- Build and generated artifacts: `.local/systemd-matrix/versions/old-enterprise/`.
- SDK implementation code was not edited.
- Live host journal state was not touched. All stock-reader checks used
  `journalctl --file`.

## Build Status

| Version | systemd/source | Built tools | Build status | Notes |
|---|---|---|---|---|
| v219 | `systemd/systemd @ 429eb63827cd` | `journalctl`, `test-journal`, `test-journal-verify` | built | Scratch-only modern-toolchain fixes were needed under `.local`: `renameat2` declaration, `sys/socket.h`/`sys/sysmacros.h`, generated gperf header dependencies, and `size_t` gperf prototypes. XZ, LZ4, and GCRYPT were enabled. |
| v239 | `systemd/systemd @ 7adc1662719f` | `journalctl`, `test-journal`, `test-journal-verify` | built | Scratch-only Meson compatibility fix was needed under `.local`: old `debug` option renamed to avoid the reserved Meson option. XZ, LZ4, and GCRYPT were enabled. |

`systemd-journal-remote` was not used for this workstream. The v219 minimal
target linked `journalctl` successfully but did not link `systemd-journal-remote`
in the reduced build; the v239 build disabled remote support to keep the scope
limited to local journal file generation and validation.

## Generated Corpus

Each version generated four 8 MiB `.journal` files:

- regular, offline;
- compressed, offline;
- regular, archived;
- compressed, online.

Synthetic corpus coverage per file:

- 7 entries;
- 39 payloads;
- 8,960 payload bytes;
- repeated field names in one entry;
- one binary payload;
- one empty value;
- one large compressible value, 8,212 bytes as a full payload;
- hash-chain pressure values;
- two boot IDs for multi-boot traversal coverage;
- deterministic realtime, monotonic, and seqnum behavior.

## Validation Summary

All validation commands passed for all 8 generated files:

- old-version `journalctl --verify --file`;
- old-version `journalctl --file --output=export --all --no-pager`;
- stock `journalctl --verify --file`;
- stock `journalctl --file --output=export --all --no-pager`;
- Rust SDK corpus digest helper;
- Go SDK corpus digest helper.

Current stock `journalctl`, Rust SDK, and Go SDK produced the same canonical
logical digest for every file:

```text
af7a90eed5d9b803c411065a615a896ef263e0e421b25070cdd5e92869eae00a
```

Old-version `journalctl -o export` produced matching counts but a different
canonical logical digest for every file:

```text
0a63eb608b716ba91c4d1835702774ef6950446c44a9a554262bdc7424480c9e
```

Discrepancy code:

- `HISTORICAL_JOURNALCTL_EXPORT_OMITS_SEQNUM`: v219/v239 `journalctl -o export`
  omits `__SEQNUM` metadata that current stock `journalctl`, Rust, and Go expose
  through this canonical digest schema. Entry and payload counts match.

## File Matrix

| Artifact | State | Compression flag | File SHA-256 | Entries | Payloads | Current/Rust/Go digest | Legacy export digest | Status |
|---|---|---|---|---:|---:|---|---|---|
| `v219/v219-compressed-offline.journal` | offline | `0x2` LZ4 | `d69d2028d3a62e9b5623d079267207cdaf4664257589eb8341281e87899f593d` | 7 | 39 | match | legacy metadata mismatch | commands pass |
| `v219/v219-compressed-online.journal` | online | `0x2` LZ4 | `ffdc027a56974cbb374cef6b3c80dcfb1d17e7a2b96da31cb387e9e361d2dbc5` | 7 | 39 | match | legacy metadata mismatch | commands pass |
| `v219/v219-regular-archived.journal` | archived | `0x0` none | `c765f51dedca13e2bf87c816e7711e0a523d7c7379757ea600c0239631027d2e` | 7 | 39 | match | legacy metadata mismatch | commands pass |
| `v219/v219-regular-offline.journal` | offline | `0x0` none | `3cfd3752ae59d0a58966495b8cb25d724931bfcd990b619415be797f06a9f310` | 7 | 39 | match | legacy metadata mismatch | commands pass |
| `v239/v239-compressed-offline.journal` | offline | `0x2` LZ4 | `f4ac05183c7d57610b7223b88b659dd03a0470b73aa6d3141b17a0a2c4f73ae8` | 7 | 39 | match | legacy metadata mismatch | commands pass |
| `v239/v239-compressed-online.journal` | online | `0x2` LZ4 | `314aeade0042ad3ccdbc7f28a57f58b4352146421142442675bada958193f3b4` | 7 | 39 | match | legacy metadata mismatch | commands pass |
| `v239/v239-regular-archived.journal` | archived | `0x0` none | `98e424ffcdbe88fc8bc0892249e48bfca07fcf93fc3cfe65e495d6864c11517c` | 7 | 39 | match | legacy metadata mismatch | commands pass |
| `v239/v239-regular-offline.journal` | offline | `0x0` none | `f786969085587b2196df9976c6f4c498d261675ff67d4173125efdb4f58653bf` | 7 | 39 | match | legacy metadata mismatch | commands pass |

## Report Artifacts

- JSON report:
  `.local/systemd-matrix/versions/old-enterprise/reports/old-enterprise-v219-v239-validation.json`
- Markdown report:
  `.local/systemd-matrix/versions/old-enterprise/reports/old-enterprise-v219-v239-validation.md`
- Generated corpus:
  `.local/systemd-matrix/versions/old-enterprise/corpus/`
- Build logs:
  `.local/systemd-matrix/versions/old-enterprise/logs/`

## Limitations

- The shared `tests/systemd_matrix` framework was absent, so this report does
  not add a reusable matrix runner.
- FSS/sealed files were not generated in this old-enterprise slice. The
  assigned coverage was regular journals, compression on/off, offline/online/
  archived states, and field/content edge cases.
- The old-version export metadata discrepancy is recorded as a compatibility
  observation, not as a Rust or Go reader failure, because current stock
  `journalctl`, Rust, and Go agree.
