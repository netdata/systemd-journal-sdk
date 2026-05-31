# SOW-0075 - VM Historical systemd Validation

## Status

Status: open

`completed` is the successful terminal status. `done` is a directory name, not a status value. Do not use `Status: done` or `Status: complete`.

Sub-state: tracked from the parallel validation discussion; activation requires explicit approval for VM provisioning because the work creates resources outside this repository.

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

- No pending SOW previously tracked the VM validation stream.
- No VM provisioning for this purpose has been started under this SOW.
- Existing source-built systemd matrix reports are useful but not a replacement for distro/runtime validation.

Risks:

- VM creation writes outside this repository and can affect host resources.
- Enterprise images may require credentials, subscriptions, or manual download steps.
- Raw VM journals can still contain sensitive data, even when generated from disposable systems.
- Distro-patched systemd behavior may expose real discrepancies requiring follow-up SOWs rather than quick fixes.

## Pre-Implementation Gate

Status: needs-user-decision

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

Sensitive data handling plan:

- Treat all VM logs and generated journals as sensitive.
- Prefer synthetic journal messages and deterministic generated payloads.
- Durable reports may include distro label, systemd version, feature flags, counts, hashes, and status codes only.
- Do not commit raw journal files, raw fields, passwords, SSH keys, RHEL account data, subscription data, machine IDs, boot IDs, IP addresses, or hostnames.

Implementation plan:

1. Present a VM target matrix and provisioning plan for user approval.
2. Provision approved VMs using the workstation's standard VM conventions.
3. Generate controlled journal cases per VM.
4. Copy outputs or compute remote digests without committing raw journals.
5. Run stock and SDK reader validation and produce sanitized reports.
6. Map discrepancies to follow-up SOWs.

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

1. VM target matrix and image sources.
2. Approval to provision VMs outside this repository.
3. Whether to commit any generated sanitized fixture, or reports only.

## Implications And Decisions

1. 2026-06-01 tracking decision
   - Decision: create this pending SOW so the VM validation stream is not forgotten.
   - Implication: implementation is not authorized yet; VM creation still requires explicit activation approval.

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

## Validation

Acceptance criteria evidence:

- Pending.

Tests or equivalent validation:

- Pending.

Real-use evidence:

- Pending.

Reviewer findings:

- Pending.

Same-failure scan:

- Pending.

Sensitive data gate:

- This planning artifact contains no raw VM logs, credentials, hostnames, IPs, machine IDs, boot IDs, or journal payloads.

Artifact maintenance gate:

- AGENTS.md: no update needed for tracking.
- Runtime project skills: no update needed for tracking.
- Specs: no update needed until implementation changes compatibility claims.
- End-user/operator docs: no update needed for tracking.
- End-user/operator skills: no update needed for tracking.
- SOW lifecycle: created as `Status: open` under `.agents/sow/pending/`.
- SOW-status.md: updated to list this pending SOW.

Specs update:

- No spec update needed for tracking only.

Project skills update:

- No project skill update needed for tracking only.

End-user/operator docs update:

- No docs update needed for tracking only.

End-user/operator skills update:

- No output/reference skill update needed.

Lessons:

- Follow-up work that is only mentioned in a completed SOW can be missed; it needs its own pending SOW or explicit rejection.

Follow-up mapping:

- Tracked by this SOW.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and later testing or use found broken behavior. Use a dated `## Regression - YYYY-MM-DD` heading at the end of the file. Never prepend regression content above the original SOW narrative.
