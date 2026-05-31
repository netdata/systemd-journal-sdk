# Mid-Era systemd Version Matrix Report

- schema: `systemd-journal-sdk-systemd-matrix-mid-era-v1`
- generated_at: `2026-05-31T17:27:30Z`
- selected v245/v247 representative: `v247`
- v247 rationale: Debian 11 era target; v245 was not attempted because the assignment allowed one of v245/v247 and v247 built successfully with the same local host-toolchain compatibility patches used for v246.
- latest stock journalctl: `systemd 260 (260.1-2-manjaro)`
- total cases: `10`
- generated files: `10`
- passed cases: `10`
- failed cases: `0`

Sensitive-data gate: generated journals are synthetic. This durable report
stores only version identities, case names, feature flags, counts, digest
prefixes, and status codes. It does not store raw journal payloads, field
values, hostnames, IPs, usernames, messages, or binary payloads.

## Versions

- `v246` `ae366f3acbc1a45504e9875099b17a7e1a221d03`: keyed hash and zstd change point; build_status=`ok`
- `v247` `4d484e14bb9864cef1d124885e625f33bf31e91c`: Debian 11 era practical representative for v245/v247 assignment; build_status=`ok`

## Cases

| case | status | state | keyed | zstd header | zstd objects | entries | version export | digest |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| v246-regular-uncompressed-keyed-offline | PASS_WITH_VERSION_EXPORT_METADATA_GAP | offline | True | False | 0 | 8 | VERSION_EXPORT_METADATA_GAP | `a160ef42b7f25221` |
| v246-regular-uncompressed-unkeyed-offline | PASS_WITH_VERSION_EXPORT_METADATA_GAP | offline | False | False | 0 | 8 | VERSION_EXPORT_METADATA_GAP | `a160ef42b7f25221` |
| v246-regular-zstd-keyed-offline | PASS_WITH_VERSION_EXPORT_METADATA_GAP | offline | True | True | 1 | 8 | VERSION_EXPORT_METADATA_GAP | `a160ef42b7f25221` |
| v246-regular-zstd-keyed-online | PASS_WITH_VERSION_EXPORT_METADATA_GAP | online | True | True | 1 | 8 | VERSION_EXPORT_METADATA_GAP | `a160ef42b7f25221` |
| v246-regular-zstd-keyed-archived | PASS_WITH_VERSION_EXPORT_METADATA_GAP | archived | True | True | 1 | 8 | VERSION_EXPORT_METADATA_GAP | `a160ef42b7f25221` |
| v247-regular-uncompressed-keyed-offline | PASS_WITH_VERSION_EXPORT_METADATA_GAP | offline | True | False | 0 | 8 | VERSION_EXPORT_METADATA_GAP | `a160ef42b7f25221` |
| v247-regular-uncompressed-unkeyed-offline | PASS_WITH_VERSION_EXPORT_METADATA_GAP | offline | False | False | 0 | 8 | VERSION_EXPORT_METADATA_GAP | `a160ef42b7f25221` |
| v247-regular-zstd-keyed-offline | PASS_WITH_VERSION_EXPORT_METADATA_GAP | offline | True | True | 1 | 8 | VERSION_EXPORT_METADATA_GAP | `a160ef42b7f25221` |
| v247-regular-zstd-keyed-online | PASS_WITH_VERSION_EXPORT_METADATA_GAP | online | True | True | 1 | 8 | VERSION_EXPORT_METADATA_GAP | `a160ef42b7f25221` |
| v247-regular-zstd-keyed-archived | PASS_WITH_VERSION_EXPORT_METADATA_GAP | archived | True | True | 1 | 8 | VERSION_EXPORT_METADATA_GAP | `a160ef42b7f25221` |
