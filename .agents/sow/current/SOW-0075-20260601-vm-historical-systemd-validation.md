# SOW-0075 - VM Historical systemd Validation

## Status

Status: in-progress

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: partial validation complete; blocked on the exhausted four-new-VM cap
and an optional Python-reader discrepancy from Ubuntu 18.04 archived files.

## Requirements

### Purpose

Build confidence that the SDK readers and writers interoperate with journal files produced by old and representative production systemd versions, using small disposable VMs rather than only synthetic source-built artifacts.

### User Request

The user asked to spawn an agent to create small VMs, with about 4 GB disks and 1 GB RAM, for interesting production systems. The VMs should use cloud-init or an equivalent mechanism for passwordless SSH access. The user also said RHEL images can be downloaded if needed and that instructions should describe what to do next per VM.

### Assistant Understanding

Facts:

- Source-built systemd matrix validation is already tracked and partly completed by SOW-0064.
- Source-built artifacts do not fully replace booted operating systems because distro patches, filesystem defaults, journald configuration, rotation behavior, machine identity, boot IDs, and active/archived transitions may differ.
- VM provisioning writes outside this repository and therefore requires explicit user approval before activation.
- Journal files copied from VMs may contain hostnames, usernames, IP addresses, command lines, or other sensitive data even if the VM is disposable.

Inferences:

- The VM work should produce a small sanitized reality corpus, not long-lived systems.
- VMs should be minimal and disposable, with deterministic journald configuration where practical.
- The first useful target set is a small representative spread: oldest practical enterprise systemd, RHEL 8/systemd 239, one mid-era distro, one recent stable distro, and one current rolling/new systemd distro.

Unknowns:

- Exact VM image list and whether enterprise images are already available locally.
- Whether RHEL subscription or image download steps are required.
- Whether VM provisioning should use existing workstation conventions, a repo-local script wrapper, or manual setup instructions only.
- Whether FreeBSD-style non-systemd portability VMs belong here or remain covered by SOW-0063.

### Acceptance Criteria

- Record an approved VM target matrix with distro, systemd version target, image source, and why each target matters.
- Provision only user-approved disposable VMs, using passwordless SSH and safe resource limits.
- Generate journal files on each VM covering at least: fresh boot, post-reboot multi-boot, compression on/off where supported, active/open and cleanly closed archived files, rotation where feasible, and at least one controlled binary-field ingestion path if supported by tooling on that VM.
- Copy generated journal files or sanitized digests back without committing raw VM logs unless explicitly approved.
- Validate each generated file with that VM's stock `journalctl --verify --file` where available, latest local stock `journalctl`, Rust reader, and Go reader.
- Record Python and Node reader results when practical, or explicitly map them to SOW-0065 if the VM matrix is used only as Rust/Go certification input.
- Produce sanitized reports with systemd version, distro, journal feature flags, counts, logical digests, verification status, and discrepancy codes.
- Do not write raw journal payloads, hostnames, IPs, usernames, generated machine IDs, passwords, keys, or secrets to durable repo artifacts.
- Cleanly document VM cleanup steps and whether any VM resources remain.

## Analysis

Sources checked:

- User discussion requesting parallel VM work.
- SOW-0064 systemd matrix and corpus validation outcome.
- SOW-0063 cross-platform portability outcome.
- Project repository-boundary and sensitive-data rules in `AGENTS.md`.

Current state:

- SOW-0075 is active in `current/`.
- Four new `sdjournal-*` VMs were created, exhausting the approved cap.
- Three VMs produced sanitized validation results: Ubuntu 18.04, Ubuntu 22.04,
  and Ubuntu 24.04.
- Debian 11 was created but blocked before validation because SSH service was
  not reachable under the minimal no-package VM profile.
- Existing `rhel810` was inspected only at the libvirt/SSH-auth level; SSH key
  authentication failed for checked users, and no modification was made.
- Existing source-built systemd matrix reports remain useful but not a
  replacement for distro/runtime validation.

Risks:

- VM creation writes outside this repository and can affect host resources.
- Enterprise images may require credentials, subscriptions, or manual download steps.
- Raw VM journals can still contain sensitive data, even when generated from disposable systems.
- Distro-patched systemd behavior may expose real discrepancies requiring follow-up SOWs rather than quick fixes.
- The default libvirt image path was observed on a filesystem above the 90%
  usage safety rail before provisioning, although the created VM disks stayed
  at the approved 4 GiB cap.
- The four-new-VM cap is exhausted; no replacement target can be created unless
  the user explicitly approves cleanup/replacement or raises the cap.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Current confidence comes from SDK tests, source-built systemd matrices, real local corpus checks, and one RHEL 8.10 investigation. That is strong but still misses distro-patched, booted-system journald behavior across representative production systems.

Evidence reviewed:

- SOW-0064 records the parallel work decision that included VM corpus validation.
- SOW-0063 records native OS portability validation, but not systemd journal generation on historical Linux distros.
- SOW-0073 records a real RHEL 8.10/systemd 239 historical reader issue.

Affected contracts and surfaces:

- Historical reader compatibility claims.
- Writer/read interoperability with stock `journalctl` on distro systemd builds.
- Validation tooling and reports under `tests/systemd_matrix/` or a sibling VM-matrix harness.
- SOW-status.md and follow-up SOW mapping.

Existing patterns to reuse:

- `tests/systemd_matrix/` report schema and sanitized digest conventions.
- `tests/corpus_eval/` canonical digest and sensitive-reporting model.
- Existing VM provisioning conventions available to the workstation, if user-approved at activation time.

Risk and blast radius:

- High operational blast radius if VM provisioning is done carelessly; no VM creation should happen until activation approval records the allowed target directory, VM names, disk/network conventions, and cleanup expectations.
- Medium compatibility blast radius if distro files expose reader/writer gaps.
- User-approved cap: at most four new disposable `sdjournal-*` VMs, each
  defaulting to 1 vCPU, 1 GiB RAM, and 4 GiB disk; if a target cannot work
  under these limits, stop and report instead of increasing resources.
- VM operations must be additive only for the explicit `sdjournal-*` domain
  names, disks, and seed ISOs. Existing domains, networks, storage pools, host
  services, global libvirt configuration, autostart settings, and host package
  state must not be modified.

Sensitive data handling plan:

- Treat all VM logs and generated journals as sensitive.
- Prefer synthetic journal messages and deterministic generated payloads.
- Durable reports may include distro label, systemd version, feature flags, counts, hashes, and status codes only.
- Do not commit raw journal files, raw fields, passwords, SSH keys, RHEL account data, subscription data, machine IDs, boot IDs, IP addresses, or hostnames.

Implementation plan:

1. Define a capped VM target matrix with at most four `sdjournal-*` VMs and
   official image/source evidence.
2. Inspect host prerequisites read-only; if required tools/images are missing,
   stop and report instead of installing packages or changing host state.
3. Provision only targets that fit the approved 1 vCPU, 1 GiB RAM, 4 GiB disk
   default and use additive VM artifacts only.
4. Generate controlled journal cases per VM.
5. Copy outputs under `.local/sow-0075/` only, or compute remote digests when
   copying is unnecessary; do not commit raw journals.
6. Run stock and SDK reader validation and produce sanitized reports.
7. Map discrepancies to follow-up SOWs or regressions.

Validation plan:

- VM stock `journalctl --version` and `journalctl --verify --file` evidence.
- Latest local stock `journalctl --file --output=export --all --no-pager` logical digest.
- Rust and Go SDK digest parity.
- Optional Python and Node parity where practical.
- `.agents/sow/audit.sh` and `git diff --check`.

Artifact impact plan:

- AGENTS.md: no expected update unless VM provisioning becomes a durable repo workflow.
- Runtime project skills: no expected update unless a reusable VM validation skill is created.
- Specs: update historical compatibility scope if VM results change claims.
- End-user/operator docs: likely unaffected unless support matrix claims change.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: pending until explicitly activated; any discrepancies become follow-up SOWs or regression reopenings.
- SOW-status.md: update now and on completion.

Open-source reference evidence:

- None checked for this tracking SOW. Implementation should use official distro image documentation and systemd source evidence where relevant.

Open decisions:

1. VM target matrix and image sources are implementation choices bounded by
   the approved four-VM cap and official-source verification.
2. VM provisioning outside this repository is approved only for additive
   `sdjournal-*` VM disks, seed ISOs, and libvirt domain definitions.
3. Durable repository artifacts must be sanitized reports/scripts only. Raw
   VM-generated journals remain under `.local/sow-0075/` and are not staged.

## Implications And Decisions

1. 2026-06-01 tracking decision
   - Decision: create this pending SOW so the VM validation stream is not forgotten.
   - Implication: implementation is not authorized yet; VM creation still requires explicit activation approval.

2. 2026-06-01 implementation routing and VM cap decision
   - Decision: activate SOW-0075 for local implementation in this workspace.
   - Decision: allow at most four new disposable VMs with names prefixed
     `sdjournal-`.
   - Decision: target resources per new VM are 1 vCPU, 1 GiB RAM, and 4 GiB
     disk by default. If a target cannot work with these limits, stop and
     report; do not silently increase resources.
   - Decision: do not modify, stop, reboot, destroy, undefine, reconfigure, or
     otherwise affect any existing VM/domain, network, storage pool, or host
     service. Do not enable autostart, make global libvirt changes, install
     host packages, or change host configuration.
   - Decision: outside-repository writes are allowed only for additive VM
     artifacts for the explicit `sdjournal-*` domains: disks, seed ISOs, and
     libvirt domain definitions.
   - Decision: existing VM `rhel810` may be inspected over SSH read-only for
     journal/systemd validation if reachable, but must not be modified without
     later explicit approval.
   - Implication: the VM plan must fit within the cap, use only official image
     evidence, keep raw journals out of durable artifacts, and stop on missing
     tooling or credential/image blockers.

3. 2026-06-01 capped target matrix
   - Decision: use a production-oriented spread that fits the 4 GiB disk cap:
     Ubuntu 18.04 LTS (`systemd 237` target), Debian 11 bullseye (`systemd
     247` target), Ubuntu 22.04 LTS (`systemd 249` target), and Ubuntu 24.04
     LTS (`systemd 255` target).
   - Evidence: official image URLs and checksums are recorded in
     `tests/vm_matrix/run_vm_matrix.py` and summarized in
     `tests/vm_matrix/reports/sow-0075-provisioning-report.md`.
   - Decision: exclude Rocky Linux 8 because the official checked image has a
     10 GiB virtual disk, violating the approved 4 GiB disk cap.
   - Decision: exclude Debian 13 after Debian 11 consumed one VM slot and
     blocked; replacing Debian 13 would exceed the four-new-VM cap.
   - Implication: this SOW provides strong Ubuntu distro coverage plus a
     blocked Debian attempt, but it does not complete the intended Debian/RHEL
     clone validation without a user decision on cleanup/replacement.

## Plan

1. Define VM targets and approval checklist.
2. Provision and generate controlled journals.
3. Validate against stock and SDK readers.
4. Produce sanitized report and follow-up mapping.

## Delegation Plan

Implementer:

- To be decided when activated. This SOW is suitable for a dedicated agent because it involves provisioning, remote validation, and report collection.

Reviewers:

- Reviewer pool after complete implementation and local validation: minimax, kimi, qwen, glm, and mimo.

Repository boundary block for every external-agent prompt:

```text
CRITICAL REPOSITORY BOUNDARY:
- DO NOT MAKE CHANGES OUTSIDE THIS REPOSITORY FOR ANY REASON.
- Repository path: current repository root.
- You may inspect external references read-only when the task requires it.
- Write, edit, delete, move, reset, checkout, install, generate, cache, or format nothing outside this repository.
- The only write exception outside the repository is /tmp.
- Prefer .local/ inside this repository for scratch work, generated temporary files, cloned references, logs, and working notes.
```

Failure handling:

- If VM provisioning requires credentials, subscriptions, or image downloads, stop and ask the user.
- If raw logs are needed to debug a discrepancy, keep them outside durable artifacts and ask for an approved secure handling path.

## Execution Log

### 2026-06-01

- Created this pending SOW to track the previously discussed VM historical-systemd validation stream.
- Moved this SOW to `current/`, set status to `in-progress`, and recorded the
  user-approved VM routing and hard caps before provisioning or implementation.
- Added `tests/vm_matrix/run_vm_matrix.py`, a repo-local VM matrix harness that
  keeps raw journals under `.local/sow-0075/` and emits sanitized reports under
  `tests/vm_matrix/reports/`.
- Verified host prerequisites read-only: required tools were present, `br0` was
  up, target `sdjournal-*` names were initially unused, and existing `rhel810`
  was running.
- Verified official image availability and checksums for Ubuntu 18.04, Debian
  11, Ubuntu 22.04, Ubuntu 24.04, Debian 13, and Rocky Linux 8 candidates.
- Created four new VMs, all with 1 vCPU, 1 GiB RAM, 4 GiB disk, and autostart
  disabled: `sdjournal-ubuntu1804`, `sdjournal-debian11`,
  `sdjournal-ubuntu2204`, and `sdjournal-ubuntu2404`.
- Generated controlled journal cases on Ubuntu 18.04, Ubuntu 22.04, and Ubuntu
  24.04: compression on/off, active/open snapshots, archived/rotated files,
  and post-reboot active/archived files.
- Debian 11 blocked before journal generation because SSH service was not
  reachable under the minimal no-package VM profile; no further VM was created
  because the four-new-VM cap was exhausted.
- Existing `rhel810` read-only SSH metadata check was attempted; SSH key
  authentication failed for checked users, and no modification was made.

## Validation

Acceptance criteria evidence:

- Target matrix recorded in
  `tests/vm_matrix/reports/sow-0075-provisioning-report.md`.
- Provisioned only user-approved disposable `sdjournal-*` VMs, capped at
  1 vCPU, 1 GiB RAM, and 4 GiB disk. Autostart is disabled for all four.
- Generated journals from three reachable VMs, six files per VM:
  `compress-on-active`, `compress-on-archived`, `compress-off-active`,
  `compress-off-archived`, `post-reboot-active`, and
  `post-reboot-archived`.
- VM stock `journalctl --verify --file` and host stock
  `journalctl --verify --file` passed for all 18 collected files.
- Host stock, Rust, Go, and Node reader logical digests matched for all 18
  collected files.
- Python reader logical digests matched 16/18 files and mismatched only two
  Ubuntu 18.04 archived files. The discrepancy code is
  `PYTHON_DIGEST_MISMATCH`.
- Raw VM journals are under `.local/sow-0075/raw/` only and were not staged.
- Debian 11 and `rhel810` blockers are recorded in the provisioning report.

Tests or equivalent validation:

- `python3 -m py_compile tests/vm_matrix/run_vm_matrix.py` passed.
- `python3 tests/vm_matrix/run_vm_matrix.py preflight` passed before initial
  provisioning. A second scoped preflight passed for the Ubuntu 22.04 and
  Ubuntu 24.04 replacement path.
- `python3 tests/vm_matrix/run_vm_matrix.py collect --targets ubuntu1804 ubuntu2204 ubuntu2404` passed and copied 18 raw journal files under `.local/`.
- `python3 tests/vm_matrix/run_vm_matrix.py validate --targets ubuntu1804 ubuntu2204 ubuntu2404 --report-json tests/vm_matrix/reports/sow-0075-vm-matrix-report.json --report-md tests/vm_matrix/reports/sow-0075-vm-matrix-report.md` completed with status `discrepancy` and discrepancy `PYTHON_DIGEST_MISMATCH`.

Real-use evidence:

- Booted distro-generated files were collected from:
  - Ubuntu 18.04 LTS, observed `systemd 237`.
  - Ubuntu 22.04 LTS, observed `systemd 249 (249.11-0ubuntu3.20)`.
  - Ubuntu 24.04 LTS, observed `systemd 255 (255.4-1ubuntu8.15)`.
- Each reachable VM generated active/open and rotated archived files before and
  after a reboot.
- Stock VM-side and host-side `journalctl --verify --file` passed for each
  collected file.

Reviewer findings:

- Not run by this implementation worker. External reviewer runs were not
  explicitly requested for this worker turn, and this SOW remains
  `in-progress` due blockers/discrepancy.

Same-failure scan:

- Same-failure search in the generated report found the Python mismatch only on
  Ubuntu 18.04 `compress-on-archived` and `compress-off-archived`; Ubuntu
  18.04 active/post-reboot files and all Ubuntu 22.04/24.04 files passed.

Sensitive data gate:

- Durable artifacts were checked for VM IP addresses and raw generated payload
  content. Reports contain sanitized aliases, versions, counts, hashes, status
  codes, and discrepancy codes only.
- Raw copied VM journals remain under `.local/sow-0075/raw/` and are not
  staged.
- SSH known-hosts data and state with encoded IPs remain under
  `.local/sow-0075/` and are not staged.

Artifact maintenance gate:

- AGENTS.md: no update needed; the VM cap was SOW-specific and does not change
  project-wide guardrails.
- Runtime project skills: no update needed yet; the harness is SOW-local and
  not yet a mandatory reusable workflow.
- Specs: no compatibility claim is changed while `PYTHON_DIGEST_MISMATCH` and
  Debian/RHEL blockers remain open.
- End-user/operator docs: no update needed; no public SDK behavior changed.
- End-user/operator skills: no output/reference skill update needed.
- SOW lifecycle: remains `Status: in-progress` under `.agents/sow/current/`.
- SOW-status.md: updated to list this active SOW.

Specs update:

- No spec update yet because this SOW did not close and did not change public
  compatibility claims.

Project skills update:

- No project skill update yet. If this VM matrix becomes a reusable release
  gate, update a project skill after the blockers are resolved.

End-user/operator docs update:

- No docs update needed; no public SDK usage changed.

End-user/operator skills update:

- No output/reference skill update needed.

Lessons:

- Follow-up work that is only mentioned in a completed SOW can be missed; it needs its own pending SOW or explicit rejection.
- Minimal no-package cloud-init is enough for Ubuntu cloud images but was not
  enough for Debian 11 SSH access in this environment. A future Debian target
  needs either an image/profile that proves SSH readiness under the cap or an
  explicit user decision to allow package installation inside the VM.
- Some cloud images expose only IPv6 link-local addresses on `br0`; the harness
  now supports `fe80::*%br0` SSH/SCP paths.
- Official image virtual disk size matters, not only download size. Rocky Linux
  8 was excluded because its official image was 10 GiB virtual size.

Follow-up mapping:

- `PYTHON_DIGEST_MISMATCH` on Ubuntu 18.04 archived files maps to SOW-0065
  unless the user asks to open a focused Python historical-archived-reader SOW.
- Debian 11 and RHEL/RHEL-clone VM validation remains blocked by the exhausted
  four-new-VM cap and missing SSH access. A user decision is needed before
  cleanup/replacement or cap increase.

## Outcome

Partial. Ubuntu 18.04, Ubuntu 22.04, and Ubuntu 24.04 VM-generated journals were
validated with sanitized reports. The SOW remains `in-progress` because Debian
11 blocked, `rhel810` SSH auth failed read-only inspection, and Python reader
parity has a historical archived-file discrepancy.

## Lessons Extracted

See `Lessons` under `## Validation`.

## Followup

- Resolve or explicitly defer `PYTHON_DIGEST_MISMATCH`.
- Decide whether to clean up/recreate the blocked Debian 11 VM or raise/adjust
  the four-new-VM cap for Debian/RHEL-family coverage.
- Decide whether existing `rhel810` can be accessed with an approved key/user or
  whether RHEL 8 coverage should use a new approved disposable target later.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and later testing or use found broken behavior. Use a dated `## Regression - YYYY-MM-DD` heading at the end of the file. Never prepend regression content above the original SOW narrative.
