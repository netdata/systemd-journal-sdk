# VM Historical systemd Matrix

This harness supports SOW-0075. It provisions at most four disposable
`sdjournal-*` VMs, generates controlled synthetic journal activity inside the
VMs, copies raw journals only under `.local/sow-0075/`, and writes sanitized
reports.

The harness must not touch the workstation live journal. Host-side stock
validation uses `journalctl --file` against copied files only. VM-side journal
generation is limited to disposable `sdjournal-*` guests created for this SOW.

## Safety Contract

- VM names are fixed and project-scoped: `sdjournal-ubuntu1804`,
  `sdjournal-debian11`, `sdjournal-ubuntu2204`, and `sdjournal-ubuntu2404`.
- Each new VM is capped at 1 vCPU, 1 GiB RAM, and 4 GiB disk.
- Existing domains, networks, storage pools, host services, autostart settings,
  and host package state must not be modified.
- SSH known-hosts data is written under `.local/sow-0075/known_hosts`.
- Durable reports contain only aliases, distro labels, systemd versions, feature
  classes, byte counts, hashes, command hashes, status codes, and discrepancy
  codes.
- Raw journals, IP addresses, hostnames, machine IDs, boot IDs, usernames, and
  payload values must not be committed.

## Commands

Preflight only:

```bash
python3 tests/vm_matrix/run_vm_matrix.py preflight
```

Provision all capped targets:

```bash
python3 tests/vm_matrix/run_vm_matrix.py provision --targets all
```

Generate and collect VM journals:

```bash
python3 tests/vm_matrix/run_vm_matrix.py collect --targets all
```

Validate copied journals and write sanitized reports:

```bash
python3 tests/vm_matrix/run_vm_matrix.py validate --targets all \
  --report-json tests/vm_matrix/reports/sow-0075-vm-matrix-report.json \
  --report-md tests/vm_matrix/reports/sow-0075-vm-matrix-report.md
```

Run the full flow:

```bash
python3 tests/vm_matrix/run_vm_matrix.py run --targets all
```

## Report Schema

JSON reports use:

```text
schema: systemd-journal-sdk-vm-matrix-v1
```

Each case records sanitized target metadata, VM-side stock `journalctl`
verification status, host-side stock verification status, Rust and Go reader
logical digests, and Python/Node reader results when their command paths are
available.

Discrepancy codes:

- `TARGET_SKIPPED`: target did not run because preflight found a cap or safety
  problem.
- `IMAGE_TOO_LARGE_FOR_CAP`: source image virtual disk exceeds 4 GiB.
- `MISSING_TOOL`: required host tool is missing.
- `DOMAIN_EXISTS`: a target `sdjournal-*` domain already exists.
- `BRIDGE_MISSING`: host bridge `br0` is unavailable.
- `VM_STOCK_VERIFY_FAILED`: VM stock `journalctl --verify --file` failed.
- `HOST_STOCK_VERIFY_FAILED`: host stock `journalctl --verify --file` failed.
- `STOCK_READ_FAILED`: host stock `journalctl --file -o export` digest failed.
- `RUST_READ_FAILED`: Rust digest helper failed.
- `GO_READ_FAILED`: Go digest helper failed.
- `PYTHON_READ_FAILED`: Python file-backed export failed.
- `NODE_READ_FAILED`: Node file-backed export failed.
- `<READER>_DIGEST_MISMATCH`: a reader digest differs from the host stock
  baseline.
