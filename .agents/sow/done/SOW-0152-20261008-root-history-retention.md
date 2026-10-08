# SOW-0152 - Root History Retention

## Status

Status: completed

Sub-state: implementation, validation and independent review complete at 41273e1; local only, no release or push.

## Requirements

### Purpose
Provide the Go SDK prerequisite for source-owned history retention across machine identities.

### User Request
The user approved the root retention design on 2026-10-08: shared age/byte allowance, full file lengths including preallocation, whole-file tail saved-time expiration, fixed caller-selected 24-hour rotation span, no forced hourly archives, cleanup outcomes independent from healthy writes, and compact factual status. Existing consumers retain default semantics. This is the Go-only prerequisite; no Rust parity or release is promised here.

### Assistant Understanding
Facts: default Log retention is machine-local, committed-byte and head-time based. Reader.OpenFile expands offsets; internal header parsing does not. Writer.archiveTo supplies existing archive durability and failure semantics.
Inferences: an explicit strict-naming Log opt-in plus standalone header inventory is the narrow reusable owner surface.
Unknowns: no product decisions remain; portable runtime validation availability will be reported.

### Acceptance Criteria
- Explicit root/source inventory validates identities, filenames, state and header extents without visiting records; rejects unsafe/quarantined candidates before pruning.
- Full file lengths and tail times govern maintenance across current and retained identities; retained active finalization uses existing writer internals.
- Current live files stay protected except idle expiration or required policy transition; next file remains lazy.
- Valid policy replacement precedes maintenance; invalid policy leaves files and policy unchanged. Safe cleanup failures do not fail appends; uncertain mutation remains fatal.
- Existing defaults/file format remain unchanged; real file fixtures, benchmark, docs/spec, audit and independent review support delivery.

## Analysis

Sources checked: go/journal/log.go, log_retention.go, writer.go, header_validation.go, current product-scope.md, project skills, pending/current SOW inventories, approved consumer design and evidence.
Current state: root scope is absent; file metadata is available through readAppendHeader/parseHeader and arena validation. Current retention errors escape through rotation before append. Current SOW queue is empty; pending integration, timestamp-lane, reader and Rust performance work has no overlap with this bounded opt-in.
Risks: deleting unowned or corrupt evidence, accidental stale-policy enforcement, archive failure suppression, per-record inventory cost, and reader behavior after unlink.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:
- Existing retention intentionally owns only one machine directory and uses committed size/head time. Consumers needing an exclusive root cannot get correct aggregate maintenance by repeatedly closing a Log.

Evidence reviewed:
- log.go open/rotate/archive lifecycle; log_retention.go default policy; writer.go append-open/archive/release; native header validation; current and pending SOWs; product-scope retention contracts; approved consumer design evidence.

Affected contracts and surfaces:
- Additive Go Log configuration and root inventory/maintenance APIs, Go tests, published Go writer docs and product scope. No on-disk format or default consumer behavior change.

Existing patterns to reuse:
- parseHeader/readAppendHeader, validateDeclaredArena, validateEmptyEntryMetadata, Writer.archiveTo, lifecycle events, policy validation and derived rotation.

Risk and blast radius:
- Opt-in strict source ownership only. Caller provides exclusive dedicated root and verifies retained active provenance/content before mutation. Header inventory is not full integrity verification. Validate the complete candidate set before pruning; propagate uncertain mutation as writer failure.

Sensitive data handling plan:
- Synthetic identities and generated file fixtures only. No live journals, host probes, personal data, credentials, production paths or raw external reports in durable artifacts.

Implementation plan:
1. Add strict root/source header inventory and opt-in lifecycle integration; copy policies and preserve default paths.
2. Implement retired active archival, idle expiration, policy transition and separately observable maintenance failures with tail-first/full-length eviction.
3. Add real-file regression/contract tests and benchmark; update docs/spec, run Go/race/docs/audit, commit validated changes for independent review.

Validation plan:
- Real generated journals cover retained identities, archive naming/header mismatches, quarantine, allocation, age head/tail difference, idle/size/policy boundaries, unlink failure and pinned reader snapshots. Run focused/full Go tests and race, inventory benchmark, cross-compile where available, docs validators and SOW audit. No format compatibility claim beyond exercised platforms.

Artifact impact plan:
- AGENTS.md: unchanged; no project-wide workflow change.
- Runtime project skills: clarify new explicit Go opt-in exception to legacy retention rules if needed.
- Specs: product-scope gains opt-in Go retention contract.
- End-user/operator docs: Go writer guide documents ownership, accounting, failure and policy update semantics.
- End-user/operator skills: none exist.
- SOW lifecycle: this tracked current SOW remains in-progress through independent review.
- SOW-status.md: both indexes updated on start and completion.

Open-source reference evidence:
- No external source implementation is reused. Consumer design supplied by coordinating task is user-approved evidence; local SDK code is the implementation ground truth.

Open decisions:
- Resolved by user approval on 2026-10-08. API spelling is an implementation choice. No push, publication or release authorized.

## Implications And Decisions

1. User approved full file-length accounting and tail-based whole-file age, explicitly neither exact TTL nor hard physical cap; size may shorten history. Caller chooses 24-hour rotation and hourly maintenance.
2. User approved dedicated root ownership across identities and prerequisite SDK PR followed by consumer adoption. Preserve unknown/corrupt/quarantined evidence. No Rust parity requirement for this opt-in.
3. Current user instructions describe native Astra delegation and require local validated commits before independent review. Applying those current instructions over the older external-reviewers harness pointers is the coordinator's interpretation. That skill was searched by the coordinator and is absent; it is not used. No pushes/history rewrites/releases.
4. Independent review and SOW completion are owned by the coordinating agent; implementation agent does not launch reviewers.

## Plan

1. Implement the approved additive root retention surface.
2. Validate real-file behavior, cost and docs; commit coherent implementation.
3. Resolve independently verified review findings, then coordinate completion.

## Implementation And Review Plan

Implementation: assigned agent owns this SDK worktree exclusively. No other repository mutations; caches and temporary files remain in this repository or /tmp.
Reviewers: independent read-only native review is authorized and coordinated by the parent after a validated implementation commit. No delegated agents are launched by the implementer.
Failure handling: record failing validation and concrete environmental limitations, fix verified source defects, and report genuine scope forks to the coordinator. Preserve uncertain file evidence.

## Execution Log

### 2026-10-08
- Read source/skills/spec and queue inventory; completed gate before source edits. Current user approval fixes the target. Existing branch is feat/root-history-retention from local master.
- Added root_retention.go inventory, policy/status and maintenance; integrated the opt-in at constructor/open/rotation/close boundaries without changing default retention. Added real-file tests and three benchmark families, Go guide/spec and compatibility skill exception.
- Real-file tests initially reproduced directory-sync-after-unlink ENOENT; fixed by syncing changed parent directories once, including partial-delete failures. A later regression test reproduced a nil writer panic when changing policy before the first lazy append. The initial flag-based fix was superseded during independent review: maintenance now compares desired/actual geometry, so a newly created writer already matches without a transition flag.
- Empty eager policy updates now discard the old empty writer and leave a lazy successor. Invalid policies are copied/validated before mutation; unsafe ownership inventory is rejected before pruning.
- Dependency lookup was unavailable with isolated empty caches; copied already-installed module data read-only into /tmp and the docs harness local cache. No dependency versions changed.
- Independent-review reproducers failed before fixes: 32 GiB to 8 MiB left a 50,331,648-byte successor with a 47,721,920-byte data hash table; 1 GiB to 8 MiB maintenance followed by either append API left two 8 MiB files without a new maintenance attempt. Public-API regression tests use default Options and verify explicit bucket/allocation overrides as well.
- Follow-up implementation retains caller allocation inputs, reuses constructor normalization for policy edits, removes pending-geometry flags, and ties cleanup readiness to the current Writer. Every detach clears that pointer to release closed-writer buffers. Rotation preserves the existing default API's behavior after cleanup errors; a regression test guards the successful append retry while that error persists. A reverted-policy test verifies no unnecessary archive when effective geometry again matches the live writer.
- Follow-up validation: focused root/Log/default-retry tests, full Go tests and full race tests pass; the root public example and all 16 wiki pages pass. Final added reverted-policy regression passes separately after those full suites. No filesystem/index/format algorithm or benchmarked inventory path changed; earlier cost evidence remains applicable, so benchmarks were not repeated. Review remains required before completion.

## Validation

Acceptance criteria evidence:
- Real file-backed tests cover previous-identity archive and active cleanup, tail-vs-head age, full 8 MiB preallocation accounting, deterministic oldest-file size eviction, shrinking/relaxing and invalid policy, policy copies, 48 idle sweeps without fragmentation, idle expiry/lazy successor, explicit 24-hour span rotation, and empty/lazy policy transitions.
- Unsafe fixture coverage: quarantine, filename SeqnumID mismatch, directory MachineID mismatch, corrupt tail range/state, truncated arena, ambiguous active/archive target, symlink, invalid machine directory and an unsafe candidate added after construction. Preserved evidence is checked.
- Synthetic unlink failure does not fail healthy append; retry succeeds. Archive-sync failure poisons subsequent writes and retains evidence. A captured IndexedSnapshot reads its original rows after unlink. Deliberately corrupt ENTRY-array content remains outside header-only inventory, demonstrating that this API does not certify indexes.

Tests or equivalent validation:
- Environment: TMPDIR=/tmp; GOCACHE=/tmp/dem-sdk-go-cache; GOMODCACHE=/tmp/dem-sdk-go-mod; GOPATH=/tmp/dem-sdk-go-path; GOPROXY=off. Caches for docs are repository-local as enforced by their harness.
- `go -C go test ./...`: pass on darwin/arm64, Go 1.27.1.
- `go -C go test -race ./...`: pass on darwin/arm64, including existing default Log tests and new root retention tests.
- Go 1.26.5 `go -C go test ./journal -run TestRoot -count=1`: pass. Exact Go 1.26.2 binary unavailable; no compiler minimum changed.
- `GOOS=windows GOARCH=amd64 CGO_ENABLED=0 go -C go test -c ./journal`: pass. Native Windows runtime unlink/rename was not tested.
- `GOOS=linux GOARCH=amd64 CGO_ENABLED=0 go -C go test -c ./journal`: pass. Native Linux/stock systemd execution was not available on this macOS host; existing systemd-dependent tests skip when tooling is absent. No file-format, live publication, or reader implementation changed.
- `python3 tests/docs/check_wiki_docs.py`: all 16 wiki pages pass.
- `python3 tests/docs/verify_examples.py --lang go`: all 20 Go examples pass, including the new root-retention example. Rust examples unchanged and not rerun.
- `bash .agents/sow/audit.sh`: clean. `git diff --check`: clean.
- `go -C go test ./journal -run '^$' -bench 'Benchmark(InspectRootRetention|RootRetentionMaintenance)' -benchmem -benchtime=200ms -count=3`: inventory is 39–56 microseconds/37 allocations/about 5 KiB for one file with 1 or 10,000 entries. Maintenance without deletion: 164–169 microseconds for one file; 1.4–1.8 ms for 20 files. The one-file 1-vs-10,000-entry comparison demonstrates independence from record count. No-deletion maintenance measurements exclude archive/unlink/fsync stalls.
- `go -C go test ./journal -run '^$' -bench BenchmarkRootRetentionRotation -benchmem -benchtime=100ms -count=3`: forced one-entry rotation 12–15 ms opt-in vs 16–18 ms default in this short filesystem/sync-dominated sample. Opt-in uses roughly 73 KiB/229–235 allocations vs 49 KiB/80 default. These noisy samples bound observed stalls and do not establish a speedup. No Rust/systemd counterpart exists for this approved Go-only root policy.

Real-use evidence:
- Tests exercise actual Create/Append/ArchiveTo/NewLog/maintenance/read-snapshot filesystem paths with synthetic records; published example compiles and runs the public policy/maintenance API. No live host journals were used.

Reviewer findings:
- Independent read-only review of c4ffeab found two verified blockers: policy changes retained normalized data-hash buckets derived from the old allowance, and a Log-lifetime creation-cleanup flag survived writer detachment, skipping maintenance on the lazy successor. Existing fixed-small-bucket tests masked the allocation defect.
- Public-API/default-Options regression tests reproduced both before source fixes, including Append and AppendRaw creation boundaries. Both now pass: derived geometry shrinks to an 8 MiB successor, and successor creation removes the older archive under the 8 MiB allowance with a new maintenance outcome. Explicit buckets/allocation size remain unchanged. The implementation retains caller allocation inputs, reuses constructor normalization, compares actual/desired geometry without transition flags, and binds readiness to the actual Writer. A default-Log rotation-cleanup-error regression verifies preserved successful append retry semantics. Focused independent recheck is required after the validated follow-up commit.
- Main-agent source inspection earlier identified the unlink-sync and empty-eager-policy issues; both remain fixed and covered. The review gate is satisfied; native Windows/Linux runtime limitations remain explicit.

Same-failure scan:
- `rg -n 'enforceRetention\(|enforceRetentionOnOpen|RootRetention' go/journal/log.go go/journal/log_retention.go`: constructor, both append shapes, rotation and close share the opt-in dispatch. Default methods retain original semantics. `ensureWriter` is the common fresh-allocation path for both Append and AppendRaw.
- `rg -n 'writer =|retentionWriter|rootPolicy|rootConfigured' go/journal/log.go go/journal/root_retention.go go/journal/log_retention.go`: all detach paths clear the retained readiness pointer; no pending allocation flag remains. Original caller allocation inputs feed the same normalization helper as construction.

Sensitive data gate:
- Durable changes contain synthetic identities and general contracts only. No raw sensitive data or workstation identities are recorded.

Artifact maintenance gate:
- AGENTS.md: unchanged; no global workflow/product default changes.
- Runtime project skills: project-journal-compatibility documents the explicit Go-only exception and header-native validation requirement.
- Specs: product-scope.md records ownership, accounting, lifecycle, errors and Go-only scope.
- End-user/operator docs: Go-API.md adds verified usage and precise limits/ownership/platform caveats.
- End-user/operator skills: none exist in this repository.
- SOW lifecycle: completed in done after independent review. Current user instructions require a validated implementation commit before review, so completion cannot be bundled in that first commit.
- SOW-status.md: canonical and convenience indexes point to the active SOW.

Specs update:
- Product scope updated with the additive contract; defaults retained.

Project skills update:
- Compatibility skill updated so its earlier default-only retention rule does not contradict this opt-in.

End-user/operator docs update:
- Go guide and verified example cover root ownership, status, policy edits, error distinction and lack of exact TTL/physical cap.

End-user/operator skills update:
- No output/reference skills exist.

Lessons:
- Directory sync after unlink must receive the parent directory. Allocation transitions compare actual writer geometry with newly derived geometry; normalized buckets must not erase the distinction between defaults and caller overrides. Creation cleanup belongs to the current writer instance, and retained readiness pointers must be cleared when writer ownership ends.

Follow-up mapping:
- Consumer adoption remains the separately approved coordinating task. Native Windows/Linux runtime compatibility limitations are reported, not silently claimed. No independent implementation is bundled or deferred.

## Outcome

Initial implementation and both verified independent-review fixes are locally validated; focused independent recheck pending.

## Lessons Extracted

Header-native inventory has stable cost as entry count grows; recovery verification remains a separate owner responsibility. The compatibility skill and Go guide capture these boundaries.

## Followup

No independent work added. Consumer adoption is owned by the coordinating task.

## Regression Log

No reopened regression; this is an explicit additive opt-in.

Completion checkpoint: the approved SDK target is complete without a DEM compatibility path. Re-review reproduced the corrected 32-GiB-to-8-MiB transition (48-MiB old allocation to 8-MiB successor) and lazy creation cleanup (one 8-MiB active, updated maintenance time). Every verified finding is fixed; no optional review item extends this scope. Completion record is a separate commit because current user instructions require implementation commits before independent review.
