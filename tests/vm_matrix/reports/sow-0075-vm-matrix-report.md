# SOW-0075 VM Historical systemd Matrix Report

- Schema: `systemd-journal-sdk-vm-matrix-v1`
- Status: `ok`
- Canonical digest schema: `systemd-journal-sdk-corpus-logical-v1`
- Python runtime: `repo-local-python-with-lz4-4.4.5`
- Discrepancies: `none`

| target | observed systemd | cases | status | discrepancy codes |
|---|---:|---:|---|---|
| ubuntu1804 | `systemd 237` | 6 | `ok` | `none` |
| ubuntu2204 | `systemd 249 (249.11-0ubuntu3.20)` | 6 | `ok` | `none` |
| ubuntu2404 | `systemd 255 (255.4-1ubuntu8.15)` | 6 | `ok` | `none` |

## Case Results

### ubuntu1804

- Distro: `Ubuntu 18.04 LTS`
- OS release: `ubuntu 18.04`
- Binary field ingestion: `logger-journald-text-fields`

| case | bytes | stock verify | reader parity | status |
|---|---:|---|---|---|
| `compress-off-active` | 8388608 | `ok` | `ok` | `ok` |
| `compress-off-archived` | 8388608 | `ok` | `ok` | `ok` |
| `compress-on-active` | 8388608 | `ok` | `ok` | `ok` |
| `compress-on-archived` | 8388608 | `ok` | `ok` | `ok` |
| `post-reboot-active` | 8388608 | `ok` | `ok` | `ok` |
| `post-reboot-archived` | 8388608 | `ok` | `ok` | `ok` |
### ubuntu2204

- Distro: `Ubuntu 22.04 LTS`
- OS release: `ubuntu 22.04`
- Binary field ingestion: `logger-journald-text-fields`

| case | bytes | stock verify | reader parity | status |
|---|---:|---|---|---|
| `compress-off-active` | 8388608 | `ok` | `ok` | `ok` |
| `compress-off-archived` | 8388608 | `ok` | `ok` | `ok` |
| `compress-on-active` | 8388608 | `ok` | `ok` | `ok` |
| `compress-on-archived` | 8388608 | `ok` | `ok` | `ok` |
| `post-reboot-active` | 8388608 | `ok` | `ok` | `ok` |
| `post-reboot-archived` | 8388608 | `ok` | `ok` | `ok` |

### ubuntu2404

- Distro: `Ubuntu 24.04 LTS`
- OS release: `ubuntu 24.04`
- Binary field ingestion: `logger-journald-text-fields`

| case | bytes | stock verify | reader parity | status |
|---|---:|---|---|---|
| `compress-off-active` | 8388608 | `ok` | `ok` | `ok` |
| `compress-off-archived` | 8388608 | `ok` | `ok` | `ok` |
| `compress-on-active` | 8388608 | `ok` | `ok` | `ok` |
| `compress-on-archived` | 8388608 | `ok` | `ok` | `ok` |
| `post-reboot-active` | 8388608 | `ok` | `ok` | `ok` |
| `post-reboot-archived` | 8388608 | `ok` | `ok` | `ok` |
