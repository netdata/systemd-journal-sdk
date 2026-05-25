# Forward Secure Sealing (FSS) Reference Vectors

This directory contains deterministic test vectors for the systemd **Forward
Secure Pseudorandom Generator (FSPRG)** used in journal Forward Secure Sealing.

## Purpose

These vectors provide a trusted, reproducible baseline that every SDK language
can use to prove it implements the same FSPRG key evolution and key extraction
as systemd v260.1 **before** integrating FSS into journal reading or writing.

## What the Vectors Prove

Each vector exercises the exact code paths used by systemd to:

1. **Generate a master key pair** (`FSPRG_GenMK`)  
   Source: `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`  
   `src/libsystemd/sd-journal/fsprg.c:239`  
   Outputs deterministic `msk` (master secret key) and `mpk` (master public key)
   bytes from a synthetic seed.

2. **Generate the initial state** (`FSPRG_GenState0`)  
   Source: `src/libsystemd/sd-journal/fsprg.c:288`  
   Derives the epoch-0 state deterministically from `mpk` and the same seed.

3. **Evolve the state** (`FSPRG_Evolve`)  
   Source: `src/libsystemd/sd-journal/fsprg.c:315`  
   Repeated squaring modulo `n` to advance the epoch counter.

4. **Seek to an arbitrary epoch** (`FSPRG_Seek`)  
   Source: `src/libsystemd/sd-journal/fsprg.c:356`  
   Uses Chinese Remainder Theorem and modular exponentiation to jump forward
   without iterating every epoch. Every epoch vector entry includes both the
   evolved state and a separately seeked state; the generator compares them
   byte-for-byte and emits `seek_matches_evolved: true` on success. If the
   comparison fails, generation aborts with a non-zero exit code.

5. **Extract sealing keys** (`FSPRG_GetKey`)  
   Source: `src/libsystemd/sd-journal/fsprg.c:405`  
   Deterministic SHA-256 expansion from the current state, indexed by `idx`.

The committed fixture includes:

- `FSPRG_RECOMMENDED_SECPAR = 1536`
- `FSPRG_RECOMMENDED_SEEDLEN = 12` bytes
- Two fixed synthetic seeds (all-zeros and incremental `0x01..0x0c`)
- Hex-encoded `msk`, `mpk`, and `state0` for each seed
- Epochs `0, 1, 2, 3, 17` with full state and 32-byte keys for `idx = 0, 1`

## Files

| File | Description |
|------|-------------|
| `fsprg_vector_generator.c` | C helper that calls systemd internal FSPRG APIs and prints JSON vectors. |
| `build.sh` | Clones (or reuses) systemd v260.1 under `.local/`, patches its build to compile the helper, and links it. |
| `run_vectors.sh` | Builds the helper, generates vectors, and compares them to the committed fixture. Supports `--update` to refresh the fixture. |
| `fixtures/fsprg-vectors-v01.json` | Committed deterministic fixture. Small, synthetic, documented. |

## Usage

### Compare mode (default)

```bash
./tests/fss/run_vectors.sh
```

Builds the generator, emits fresh vectors, and fails if they differ from the
committed fixture.

### Update mode

```bash
./tests/fss/run_vectors.sh --update
```

Overwrites `fixtures/fsprg-vectors-v01.json` with freshly generated vectors.
Only use this when the upstream systemd baseline or the generator itself has
changed intentionally.

## Safety Guardrails

**Safe commands for this repo:**

- `tests/fss/build.sh` – builds the reference helper inside `.local/`
- `tests/fss/run_vectors.sh` – runs the helper and compares fixtures
- `journalctl --verify --file <repo-local-fixture>` – stock verification against
  repository-local journal files

**Unsafe / out-of-scope commands:**

- `systemd-cat` – writes to the live host journal
- `logger` – writes to the live host journal
- `journalctl --setup-keys` – daemon-only key management
- Live `journalctl` without `--file` or a repository-local `--directory`
- `systemd-journal-remote --seal` against live journal data
- Writing to `/var/log/journal` or `/run/log/journal`
- Starting, stopping, or reloading systemd services

Daemon-only key setup, sealing intervals controlled by the systemd daemon, and
`--setup-keys` lifecycle operations are **explicitly out of scope** for this
project. The vectors and future FSS SDK work operate on file-format behavior
only, using deterministic synthetic test keys.

## Notes

- The helper is built from systemd v260.1 internal code; it is a test reference,
  not a redistributable library.
- The generator uses `_exit(0)` after `fflush(stdout)` to avoid a libgcrypt
  atexit-handler crash that occurs with dynamic gcrypt loading in this build
  configuration. All output is flushed before exit; the workaround does not
  affect vector correctness.
