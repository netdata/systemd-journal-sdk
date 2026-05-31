# Worker D Format-Change Execution Report - v252 and v254

## Scope

- SOW: `SOW-0064-20260530-real-world-journal-corpus-evaluation.md`
- Assigned versions:
  - `v252`: compact format change point.
  - `v254`: tail-entry boot ID compatibility point.
- Full sanitized case matrix:
  - `tests/systemd_matrix/format-change/worker-d-v252-v254-format-change.json`
- Scratch artifacts:
  - `.local/systemd-matrix/versions/format-change/`

## Source And Build

- `systemd/systemd @ e8dc52766e1f`
  - Tag: `v252`
  - Build status: `ok`
- `systemd/systemd @ 994c7978608a`
  - Tag: `v254`
  - Build status: `ok`
- Latest local stock journalctl:
  - `systemd 260 (260.1-2-manjaro)`

Build notes:

- Sources were extracted from the local read-only systemd mirror into `.local`.
- No live host journal state was read or written.
- The copied systemd sources needed a workstation-kernel build shim for newer filesystem magic constants.
- The copied systemd sources were patched only under `.local` to add the synthetic `test-format-change-ingester` helper.

## Matrix Result

- Attempted cases: `40`
- Generated journal files: `36`
- Version journalctl verify passes: `36 / 36 generated`
- Latest journalctl verify passes: `36 / 36 generated`
- Latest journalctl, Rust reader, and Go reader logical digest parity: `36 / 36 generated`
- FSS/sealed cases: `4 attempted, 0 generated`

Coverage in generated files:

- compact off/on: covered for v252 and v254.
- keyed hash off/on: covered for v252 and v254.
- zstd compression off/on: covered for v252 and v254.
- online/offline/archived final states: covered where generated.
- repeated fields, binary fields, large fields, empty fields, and hash-collision-oriented values: covered in every generated case.
- v254 tail-entry boot ID behavior: covered by `compact-on__keyed-on__zstd-on__sealed-off__archived__multiboot`.

## Findings

- `OK`: `18` cases.
  - All are v254 unsealed cases.
  - v254 files set compatible flag `0x2` for tail-entry boot ID.
  - The v254 multiboot case matched the expected synthetic last boot ID in the header.
- `VERSION_JOURNALCTL_EXPORT_METADATA_DRIFT`: `18` cases.
  - All are v252 unsealed cases.
  - Counts match across v252 journalctl, latest journalctl, Rust, and Go.
  - Latest journalctl, Rust, and Go logical digests match.
  - v252 journalctl export omits `__SEQNUM`, so its canonical digest differs under the SOW-0064 schema.
- `GENERATOR_FAILED`: `4` cases.
  - These are the FSS/sealed attempts, two per version.
  - The synthetic direct-FSS setup failed with `EINVAL`.
  - The run did not use or create `/var/log/journal/<machine-id>/fss`, because touching live host journal state was forbidden.

## Status

Partially implemented:

- Build, generation, and validation succeeded for all unsealed v252/v254 format-change cases.
- FSS/sealed generation remains blocked for this worker slice under the no-live-host-journal-state constraint.
