# SOW-0075 RHEL 8.10 Read-Only Validation Report

- Schema: `systemd-journal-sdk-vm-matrix-v1`
- Status: `ok`
- Target: `rhel810`
- Observed systemd: `systemd 239 (239-82.el8_10.16)`
- OS release: `rhel 8.10`
- Host stock systemd: `systemd 260 (260.1-2-manjaro)`

## Remote Journal Summary

- Total files: `8`
- Active files: `1`
- Archived files: `7`
- VM-side stock verify: `ok`

## Local Samples

| case | bytes | host stock verify | reader parity | status note |
|---|---:|---|---|---|
| `active` | 8388608 | `failed` | `not used` | live active snapshot; verify failure is not compatibility evidence |
| `archived` | 50446336 | `ok` | `ok` | stable archived sample |
