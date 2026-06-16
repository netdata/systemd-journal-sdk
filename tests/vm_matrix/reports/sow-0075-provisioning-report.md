# SOW-0075 VM Provisioning Report

- Status: `completed-with-debian-recorded-blocker`
- New VM cap: `4`
- Created new VMs: `4`
- Usable validation VMs: `3`
- Additional read-only validation VM: `rhel810`
- Raw copied journals: `.local/sow-0075/raw/` only, not staged
- Host storage risk: default libvirt image path is on a filesystem observed
  above the 90% usage safety rail before provisioning; VM disks were kept at the
  approved 4 GiB cap.

## Target Matrix

| alias | VM name | distro | expected systemd | official image evidence | cap result | outcome |
|---|---|---|---|---|---|---|
| `ubuntu1804` | `sdjournal-ubuntu1804` | Ubuntu 18.04 LTS | 237-era | Ubuntu bionic cloud image SHA256 `8dd2e6b5e5aad20c3f836123b300cba9861249408cbb07c359145a65d6bab6b6` | 2.2 GiB source virtual disk, resized to 4 GiB | validated |
| `debian11` | `sdjournal-debian11` | Debian 11 bullseye | 247-era | Debian bullseye genericcloud image SHA512 `2e9311602ce0d6a7f7e3bdaea03507de99c67aecd1b93563c5a8d5d08d16d224caa3afc867df56977631ea2fec940250a3bcdd16393fee8c4cccd1e8c1e8d3bd` | 3.0 GiB source virtual disk, resized to 4 GiB | recorded blocker accepted for closure: bridge-neighbor IP discovery works, but SSH to the seeded `user` account returns connection refused on port 22 and QEMU guest agent is not connected |
| `ubuntu2204` | `sdjournal-ubuntu2204` | Ubuntu 22.04 LTS | 249-era | Ubuntu jammy cloud image SHA256 `f6729b53d930d7f0c6691eb553cfa6be7109de9412125bf1bf2dc6747de8a44d` | 2.2 GiB source virtual disk, resized to 4 GiB | validated |
| `ubuntu2404` | `sdjournal-ubuntu2404` | Ubuntu 24.04 LTS | 255-era | Ubuntu noble cloud image SHA256 `53fdde898feed8b027d94baa9cfe8229867f330a1d9c49dc7d84465ee7f229f7` | 3.5 GiB source virtual disk, resized to 4 GiB | validated |

## Excluded Candidate

| candidate | reason |
|---|---|
| Rocky Linux 8 GenericCloud | Official image was available and checksummed, but its virtual disk was 10 GiB, which violates the approved 4 GiB disk cap. |
| Debian 13 trixie | Official image was available and checksummed, but the four-new-VM cap was consumed after Debian 11 blocked and Ubuntu 24.04 replaced the unused fourth slot. |

## Existing VM Read-Only Check

| VM | outcome |
|---|---|
| `rhel810` | Running and reachable through the configured SSH alias. Read-only journal validation is recorded in `sow-0075-rhel810-read-only-report.md`. Direct generic cloud-user checks still fail public-key authentication and are not the approved access path. No modification was made. |

## Retired Reader Dependency Finding

The original retired-reader mismatch on Ubuntu 18.04 archived files was
reproduced with the system interpreter because `lz4` was not importable. The
missing DATA objects were LZ4-compressed payloads; the retired high-level entry
path skipped those payloads after reporting that decompression was unavailable.
After creating a repo-local `.local/sow-0075/reader-venv` with the required
dependencies, retired-reader parity passed for all Ubuntu VM cases and for the
RHEL 8.10 archived sample. No host package was installed.

## Resources Remaining

All four created `sdjournal-*` domains remain running, with autostart disabled:

- `sdjournal-ubuntu1804`: 1 vCPU, 1 GiB RAM, 4 GiB disk
- `sdjournal-debian11`: 1 vCPU, 1 GiB RAM, 4 GiB disk
- `sdjournal-ubuntu2204`: 1 vCPU, 1 GiB RAM, 4 GiB disk
- `sdjournal-ubuntu2404`: 1 vCPU, 1 GiB RAM, 4 GiB disk

Cleanup is intentionally not performed by this SOW worker because the user did
not explicitly approve destroy/undefine/delete operations for the newly created
VM resources.
