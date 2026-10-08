# SOW-0152 - Root History Retention

## Status

Status: in-progress

Sub-state: reopened for Rust feature parity at user request after rebase onto local master. Prior Go validation/review remains evidence; Rust design, implementation and validation are in progress. No push or release.

## Requirements

### Purpose
Provide equivalent Go and Rust source-owned history retention across machine identities, with idiomatic APIs and unchanged default consumer behavior.

### User Request
The user approved the root retention design on 2026-10-08: shared age/byte allowance, full file lengths including preallocation, whole-file tail saved-time expiration, fixed caller-selected 24-hour rotation span, no forced hourly archives, cleanup outcomes independent from healthy writes, and compact factual status. Existing consumers retain default semantics. The user subsequently required Go/Rust parity for this branch. The earlier Go-only assumption was not a user-approved parity waiver and is superseded. Publication remains a separately authorized operation.

### Assistant Understanding
Facts: default Log retention is machine-local, committed-byte and head-time based. Reader.OpenFile expands offsets; internal header parsing does not. Writer.archiveTo supplies existing archive durability and failure semantics.
Delivered surface: explicit strict-naming Log opt-in, standalone startup inventory, and live-aware Log inventory for runtime queries/status.
Unknowns: no unresolved product decisions. Independent parity review identified a shared Rust header-bound validation gap; the repair and validation are recorded below. Native Windows/Linux runtime validation limitations remain.

### Acceptance Criteria
- Explicit root/source inventory validates identities, filenames, state and header extents without visiting records; rejects unsafe/quarantined candidates before pruning.
- Full file lengths and tail times govern maintenance across current and retained identities; retained active finalization uses existing writer internals.
- Current live files stay protected except idle expiration or required policy transition; next file remains lazy.
- Valid policy replacement precedes maintenance; invalid policy leaves files and policy unchanged. Safe cleanup failures do not fail appends; uncertain mutation remains fatal.
- Live-aware inventory rejects missing/replaced active paths and mismatched identities before maintenance; failed writers remain inspectable for status.
- Existing defaults/file format remain unchanged; real file fixtures, benchmark, docs/spec, audit and independent review support delivery.

## Analysis

Sources checked: go/journal/log.go, log_retention.go, writer.go, header_validation.go, current product-scope.md, project skills, pending/current SOW inventories, approved consumer design and evidence.
Pre-implementation state: root scope was absent; file metadata was available through readAppendHeader/parseHeader and arena validation. Retention errors escaped through rotation before append. The initial current SOW queue was empty; pending integration, timestamp-lane, reader and Rust performance work had no overlap with this bounded opt-in.
Delivered state: the opt-in covers owned identities, file lengths/tail saved times, lazy lifecycle/policy changes, separate cleanup outcomes, and live writer correspondence. Default consumers and file format remain unchanged.
Risks: deleting unowned or corrupt evidence, accidental stale-policy enforcement, archive failure suppression, per-record inventory cost, and reader behavior after unlink.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:
- Existing retention intentionally owns only one machine directory and uses committed size/head time. Consumers needing an exclusive root cannot get correct aggregate maintenance by repeatedly closing a Log.

Evidence reviewed:
- log.go open/rotate/archive lifecycle; log_retention.go default policy; writer.go append-open/archive/release; native header validation; current and pending SOWs; product-scope retention contracts; approved consumer design evidence.

Affected contracts and surfaces:
- Equivalent opt-in Go and Rust Log configuration, root inventory/maintenance APIs, native header/descriptor helpers required by Rust, real-file tests, both published writer guides, product scope and the repository parity rule. No on-disk format or default consumer behavior change.

Existing patterns to reuse:
- Go parseHeader/readAppendHeader, arena/empty-entry validation, Writer.archiveTo, lifecycle events and policy normalization. Rust JournalHeader bounds/empty-entry validation, ActiveFile/OwnedChain archive and directory-sync lifecycle, config normalization, and existing descriptor/platform dependencies. Inventory must not use JournalFile::open (maps hash tables) or Reader entry expansion.

Risk and blast radius:
- Opt-in strict source ownership only. Caller provides exclusive dedicated root and verifies retained active provenance/content before mutation. Header inventory is not full integrity verification. Validate the complete candidate set before pruning; propagate uncertain mutation as writer failure.

Sensitive data handling plan:
- Synthetic identities and generated file fixtures only. No live journals, host probes, personal data, credentials, production paths or raw external reports in durable artifacts.

Implementation plan:
1. Retain reviewed Go implementation; implement equivalent strict root/source Rust header inventory and live-descriptor identity validation.
2. Integrate Rust retired/live active finalization without eager successors, full-length/tail-time pruning, original-policy-derived geometry and distinct maintenance/writer failures. Keep readiness with each live file and preserve default semantics.
3. Add Rust real-file counterparts to Go contract/regression tests, cross-language fixture checks and bounded performance evidence; update docs/spec/rules, run affected suites/docs/audit, commit validated changes for independent review.

Validation plan:
- Real generated journals cover retained identities, archive naming/header mismatches, quarantine, allocation, age head/tail difference, idle/size/policy boundaries, unlink failure and pinned reader snapshots. Run focused/full Go tests and race, inventory benchmark, cross-compile where available, docs validators and SOW audit. No format compatibility claim beyond exercised platforms.

Artifact impact plan:
- AGENTS.md: make language feature parity explicit during development; release skill references the rule.
- Runtime project skills: document paired opt-in behavior and reference the canonical parity rule.
- Specs: product-scope records paired root retention behavior.
- End-user/operator docs: both writer guides and shared API guidance document ownership, accounting, failure and policy updates with verified examples.
- End-user/operator skills: none exist.
- SOW lifecycle: reopened in current for the parity correction, then complete only after Rust and cross-language validation/review.
- SOW-status.md: both indexes updated on start and completion.

Open-source reference evidence:
- No external source implementation is reused. Consumer design supplied by coordinating task is user-approved evidence; local SDK code is the implementation ground truth.

Open decisions:
- Resolved by user approval on 2026-10-08. API spelling is an implementation choice. No push, publication or release authorized.

## Implications And Decisions

1. User approved full file-length accounting and tail-based whole-file age, explicitly neither exact TTL nor hard physical cap; size may shorten history. Caller chooses 24-hour rotation and hourly maintenance.
2. User approved dedicated root ownership across identities and prerequisite SDK PR followed by consumer adoption. Preserve unknown/corrupt/quarantined evidence. The original implementation inferred Go-only scope from the first consumer; the user corrected that assumption and requires Rust parity in this branch.
3. Current user instructions describe native Astra delegation and require local validated commits before independent review. Applying those current instructions over the older external-reviewers harness pointers is the coordinator's interpretation. That skill was searched by the coordinator and is absent; it is not used. No pushes/history rewrites/releases.
4. Independent review and SOW completion are owned by the coordinating agent; implementation agent does not launch reviewers.

## Plan

1. Implement the approved additive root retention surface.
2. Validate real-file behavior, cost and docs; commit coherent implementation.
3. Resolve independently verified review findings, then coordinate completion.

## Implementation And Review Plan

Implementation: coordinator owns integration tests, public docs, rules, specs and SOW artifacts; a delegated implementer may exclusively own Rust runtime files and its internal unit tests. No overlapping writers. No other repository mutations; caches and temporary files remain in this repository or /tmp.
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
- Follow-up validation: focused root/Log/default-retry tests, full Go tests and full race tests pass; the root public example and all 16 wiki pages pass. Final added reverted-policy regression passes separately after those full suites. No filesystem/index/format algorithm or benchmarked inventory path changed; earlier cost evidence remains applicable, so benchmarks were not repeated. The required focused review subsequently passed, as recorded below.

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
- `go -C go test ./journal -run '^$' -bench BenchmarkRootRetentionRotation -benchmem -benchtime=100ms -count=3`: forced one-entry rotation 12–15 ms opt-in vs 16–18 ms default in this short filesystem/sync-dominated sample. Opt-in uses roughly 73 KiB/229–235 allocations vs 49 KiB/80 default. These noisy samples bound observed stalls and do not establish a speedup. This was the initial Go-only measurement before the parity correction; no matching stock-systemd custom root policy exists. Rust evidence is recorded in the parity correction below.

Real-use evidence:
- Tests exercise actual Create/Append/ArchiveTo/NewLog/maintenance/read-snapshot filesystem paths with synthetic records; published example compiles and runs the public policy/maintenance API. No live host journals were used.

Reviewer findings:
- Independent read-only review of c4ffeab found two verified blockers: policy changes retained normalized data-hash buckets derived from the old allowance, and a Log-lifetime creation-cleanup flag survived writer detachment, skipping maintenance on the lazy successor. Existing fixed-small-bucket tests masked the allocation defect.
- Public-API/default-Options regression tests reproduced both before source fixes, including Append and AppendRaw creation boundaries. Both now pass: derived geometry shrinks to an 8 MiB successor, and successor creation removes the older archive under the 8 MiB allowance with a new maintenance outcome. Explicit buckets/allocation size remain unchanged. The implementation retains caller allocation inputs, reuses constructor normalization, compares actual/desired geometry without transition flags, and binds readiness to the actual Writer. A default-Log rotation-cleanup-error regression verifies preserved successful append retry semantics. Focused independent recheck of 41273e1 passed; its coverage remains applicable.
- Main-agent source inspection earlier identified the unlink-sync and empty-eager-policy issues; both remain fixed and covered.
- Final focused read-only review by retention_sdk_review (Astra high) of bf2f1e5 found no verified blocker. Coverage included live-file identity before runtime inventory/maintenance, deletion, policy replacement and error classes, pinned readers and shutdown, with earlier SDK coverage retained. The coordinator supplied the review result and authorized completion. Native Windows/Linux runtime limitations remain explicit.

Same-failure scan:
- `rg -n 'enforceRetention\(|enforceRetentionOnOpen|RootRetention' go/journal/log.go go/journal/log_retention.go`: constructor, both append shapes, rotation and close share the opt-in dispatch. Default methods retain original semantics. `ensureWriter` is the common fresh-allocation path for both Append and AppendRaw.
- `rg -n 'writer =|retentionWriter|rootPolicy|rootConfigured' go/journal/log.go go/journal/root_retention.go go/journal/log_retention.go`: all detach paths clear the retained readiness pointer; no pending allocation flag remains. Original caller allocation inputs feed the same normalization helper as construction.

Sensitive data gate:
- Durable changes contain synthetic identities and general contracts only. No raw sensitive data or workstation identities are recorded.

Artifact maintenance gate:
- AGENTS.md: feature parity is explicitly required during development, with user-owned exceptions and acceptance evidence in both languages.
- Runtime project skills: project-journal-compatibility documents both root opt-ins and header-native validation; release skill references the canonical parity rule.
- Specs: product-scope.md records paired ownership, accounting, lifecycle and errors.
- End-user/operator docs: Go-API.md adds verified usage and precise limits/ownership/platform caveats.
- End-user/operator skills: none exist in this repository.
- SOW lifecycle: completed in done after independent review. Current user instructions require a validated implementation commit before review, so completion cannot be bundled in that first commit.
- SOW-status.md: canonical and convenience indexes record completed status and no remaining current SDK SOW.

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
- Consumer adoption is the separately approved coordinating task; its dependency pin awaits a separately authorized SDK release. SDK publication is outside this SOW, not deferred implementation. Native Windows/Linux runtime compatibility limitations are reported, not silently claimed. No independent implementation is bundled or deferred.

## Outcome

The prior Go implementation and its review fixes are complete. Branch readiness is reopened for equivalent Rust behavior and cross-language evidence. Publication/release remains separately authorized; consumer dependency pinning awaits an SDK release.

## Lessons Extracted

Header-native inventory has stable cost as entry count grows; recovery verification remains a separate owner responsibility. The compatibility skill and Go guide capture these boundaries.

## Followup

No independent work added. Consumer adoption is owned by the coordinating task.

## Regression Log

The 2026-10-08 live-writer inventory regression was reopened, reproduced, fixed in bf2f1e5, validated and independently reviewed. Its evidence follows.

Completion checkpoint: the approved SDK target is complete without a DEM compatibility path. Re-review reproduced the corrected 32-GiB-to-8-MiB transition (48-MiB old allocation to 8-MiB successor) and lazy creation cleanup (one 8-MiB active, updated maintenance time). Every verified finding is fixed; no optional review item extends this scope. Completion record is a separate commit because current user instructions require implementation commits before independent review.

## Regression - 2026-10-08

Observed failure and missed invariant:
- Moving the active machine directory outside the configured root leaves the Log holding a live file descriptor, while standalone directory inventory reports zero files and maintenance reports success. Replacing the old machine path with a regular file is also silently ignored as metadata. Consumer query/status requires a missing-file error rather than false-empty success. Initial tests inspected directory-visible files but did not challenge their correspondence with an owned live writer.

Approved repair plan and gate:
- The coordinating task authorizes a narrow SDK-owned Log.InspectRootRetention API, usable on an open failed Log, requiring root opt-in. It combines standalone header inventory with live path/filesystem identity and header-identity checks. Use it internally before maintenance mutations. Reject canonical machine-named non-directories in standalone inventory. Missing-file inventory errors do not poison the writer. No per-record filesystem checks or general concurrent-external-modification guarantee is added.
- Gate status: ready. Existing caller exclusion, recovery verification, synthetic-fixture/cache/repository boundaries and no-publication constraints remain in effect. Source files, tests, docs and product-scope are affected; other SDK consumers retain defaults. No unresolved user-owned decision remains.

Validation and artifact plan:
- Reproduce existing maintenance false-empty behavior with a public real-filesystem test before repair, then test missing active directory/file, replacement identity, recovery, standalone machine-path type rejection, and inspection after writer failure. Run affected Go tests/race and docs/audit; retain applicable earlier full-suite/benchmark evidence. Document standalone versus live-aware inventories without broadening external-modification guarantees. The coherent fix was committed before coordinated review; review is now complete.

Regression implementation and validation:
- Both initial reproducers failed before the fix: missing live directory produced valid zero inventory/success; a canonical machine-named regular file passed standalone inventory. The live-aware method now checks path presence/filesystem identity with Lstat, fd Stat and SameFile, then verifies file/machine/sequence journal identities in the shared header inventory. Every maintenance sample uses it. Neither this read-only method nor its errors calls writable/recordFailure; failed writer status remains inspectable.
- Public real-filesystem tests cover a moved machine directory and moved active file, filesystem ENOENT, invalid maintenance inventory, unchanged last-success time, healthy Sync while missing, restoration/recovery, an identical-header replacement inode, changed header identity, and non-opt-in/closed rejection. The existing real-file archive-failure test confirms inspection still works after writer failure.
- `go -C go test ./journal -run 'TestRoot|TestLog|TestDefaultLogRotation' -count=1`: pass. The same affected suite with `-race` passes. Prior full-suite evidence remains valid for unchanged reader/writer/default paths; this repair does not alter append or file format code.
- `python3 tests/docs/check_wiki_docs.py`: all 16 wiki pages pass. `python3 tests/docs/verify_examples.py --only go-root-retention`: the updated public example passes, including live-aware inspection.
- `go -C go test ./journal -run '^$' -bench BenchmarkInspectLiveRootRetention -benchtime=200ms -benchmem -count=3`: one live file with 1 or 10,000 records takes about 35–36 microseconds, about 5.3 KiB and 37 allocations per inspection on darwin/arm64. This demonstrates record-count independence only; it does not measure archive/unlink/fsync stalls or establish a speedup over previous runs.
- Code search confirms only existing maintenance boundaries invoke the new check internally; no per-record filesystem probes were added. Standalone root scanning now reserves canonical machine names for directories. Docs, product scope and the compatibility skill distinguish startup inventory from live-aware status/query inventory, with caller exclusion still required. Native Windows/Linux runtime caveats remain unchanged.
- Validation uses the same /tmp and repository-local cache redirection recorded above; no live journals, external state or unapproved repository writes. Durable artifacts contain no sensitive data. SOW audit and diff checks passed before the fix commit; completion artifacts receive the same checks.

Regression outcome:
- Fixed in bf2f1e5. The affected tests/race, docs example/wiki checks and audit passed. Focused independent review found no verified blocker and retained earlier SDK coverage; the coordinator authorized completed/done status. No push or release.

Final completion checkpoint: local SDK source is unchanged from reviewed bf2f1e5. This completion commit changes only the SOW lifecycle/current-state record and its indexes. No broad validation rerun is needed for these tracking-only edits.


## Parity Correction - 2026-10-08

User direction: keep Go/Rust parity for feat/root-history-retention and make the requirement visible in repository rules. The release skill already requires accepted functionality in both languages. The prior Go-only assumption was too narrow; it is not retained as an approved exception.

Target and scope:
- Deliver the existing root inventory, tail-time/full-length policy, live-file correspondence, lazy finalization, mutable policy and maintenance-status contract in Rust as well as Go. APIs follow each language's ownership conventions. Defaults, on-disk format and explicit caller writer exclusion remain unchanged.
- Retain the reviewed Go implementation and regression evidence; add Rust counterparts for the known allocation, successor-readiness, safe-failure and live-path failure classes.
- Make parity a development rule in AGENTS.md and reference it from release/compatibility guidance. Remove current Go-only claims from docs/specs; preserve prior validation as historical evidence.
- Release metadata, publication, external consumers and parked unrelated Rust optimization work remain outside this task. No dependencies or compiler-minimum changes are planned.

Investigation and plan:
1. Verify Rust lifecycle/header/identity primitives and the Go contract; obtain bounded independent design challenge.
2. Implement opt-in Rust APIs, root maintenance and real-file tests using existing native header/writer primitives. Preserve original caller rotation inputs; fresh active files own maintenance readiness.
3. Document both language surfaces with executable examples, root parity rules and contract/spec updates.
4. Validate affected Rust/full language suites as appropriate, Go regression evidence, cross-language real files and record-count-independent inventory cost. Commit coherent validated changes, obtain independent review, fix verified blockers, then close this SOW.

Baseline: SDK HEAD5961564 after local-master rebase; working tree clean. Root retention is absent from Rust. Rust writer/default tests are running with caches/output under /tmp. Current SOW queue is empty; pending Rust array-open optimization and legacy-core cleanup remain independent and unactivated.

Parity gate: ready. The user request fixes the end state; there is no new product/architecture fork. Rust needs narrow fixed-header and owned-descriptor identity primitives plus archive-without-successor lifecycle, using existing dependencies. Error/result spelling and helper organization are routine language-specific implementation choices. Baseline all-feature Rust log-writer suite passes. Independent design inspection is in progress and any verified constraint must be incorporated before the affected implementation.


Parity implementation and validation checkpoint:
- Rust exposes the paired opt-in, fixed-header inventory, opening-identity snapshots and live descriptor checks, value policy updates, lazy Archived events, tail/length pruning and Arc-backed typed outcomes. Startup and mutation reuse existing writer lifecycles; no on-disk format or default policy changes. Root rejects namespaces/artifact sizing. Infallible artifact-sizer builder cannot bypass validation or mutate evidence during close/drop.
- Existing source patterns exposed two regressions during self-review: resetting default-mode creation readiness changed cleanup-error retry behavior; unconditional root close maintenance pruned histories when no active existed or an empty file was discarded. Both public regression tests failed before fixes and pass afterward. A third focused unit test reproduced a retained empty recovered current active; the fix reuses startup disposal for empties while forbidding unsupported-file quarantine in root mode.
- Rust 1.91.0 (declared minimum), darwin/arm64: log-writer all-features passes 22 unit tests, 56 existing integration tests, 22 new public root tests and 2 doc tests. Core/public SDK/log-writer default-feature suites pass with two pre-existing macOS group-name assertions filtered: 165 public SDK, 86 core, 14 log unit, 56 existing integration, 22 root integration tests plus applicable docs. The two filtered tests were separately reproduced at unchanged HEAD5961564: macOS resolves GID 0 to wheel, while those reader tests require root. No unrelated reader fix is bundled.
- Full-workspace attempts hit the unchanged legacy FFI build's fresh metadata resolution: offline cache resolves yanked chacha20 0.10.1 without the workspace lock. Workspace all-features additionally needs uncached allocative 0.3.6. These broader environmental/legacy checks are not claimed passing; all changed crates and executable public examples were tested. No compiler/dependency version changes or network workaround was made. Native Windows/Linux runtime is untested; Windows code uses the existing windows-sys handle identity API with its filesystem feature enabled. Only the native macOS target is installed.
- `go -C go test ./...`: passes again; Go runtime is unchanged from the reviewed/rebased implementation. Existing race evidence remains applicable.
- Wiki validator passes all 16 pages. All 19 Go and 16 Rust marked docs examples pass, including both root APIs. An initial Rust docs-cache miss was resolved by copying existing isolated cached dependencies into the harness's repository-local cache; no dependency changes.
- Shared `tests/interoperability/run_root_retention_parity.py --benchmark` passes both writers x both inventories/readers, retained active finalization, tail-age pruning, byte-preserving malformed/quarantine startup rejection, and lazy expiry with successor sequence/readback. Its 1-versus-10000-entry header inventory measurements show no record-count growth: approximately 35-63 microseconds per inventory across Go optimized/Rust debug builds. This is a scaling check, not a release-speed comparison or an archive/unlink/fsync benchmark.
- Alternating release default-directory benchmark, 20000 rows, five before/after runs per append shape: raw median 210.6 to 194.2 ms; structured median 207.9 to 211.8 ms. Samples overlap with filesystem/scheduling noise; no consistent regression or speedup claim. Original and final binaries plus JSON evidence remain under .local/retention-validation. The new checks run at inventory/maintenance boundaries, never per entry.
- Durable docs now label root retention unreleased and call out Rust exhaustive Config/enum additions; version selection and publication remain separately authorized release work.
- Reference searches: `rg -n 'Go-only|Rust parity|Go opt-in' docs AGENTS.md .agents/skills .agents/sow/specs/product-scope.md` leaves only the release-tagging instruction about language-specific releases, which is valid. Current SOW historical Go-only references explain the corrected assumption, not an exception. `rg -n 'LogLifecycleEvent::' rust` shows existing matches retain wildcard handling and new lazy archive tests cover the added event. Retention readiness paths reset for root detach/create only; default cleanup retry remains tested.
- Independent design investigation found the mapped-header identity trap, original-policy normalization, fresh-file readiness and artifact-sizer builder bypass; all are implemented/tested. Independent final implementation review remains to be performed after the validated commit.

Parity review repair:
- Independent review of 664b3df identified a verified deletion-safety blocker: Rust's shared stable-arena validator omits ENTRY-array and tail-entry header offset bounds enforced by Go. Patching an expired archive's entry_array_offset to u64::MAX makes Go reject/preserve it, but Rust accepts inventory and deletes it during maintenance. The reproducer uses both compiled public API probes on copied synthetic fixtures.
- Repair plan: complete JournalHeader::validated_arena_end's fixed-header bounds once for retention inventory, append recovery and excluded-writer snapshot callers. Preserve ordinary live-reader mapping validation and the documented lack of object/index traversal in inventory. Add real-file rejection/evidence-preservation cases to Rust and the shared matrix; rerun affected core, public snapshot and directory writer suites. No format change or new ownership contract.
- Review also found misleading enforce_retention rustdoc claiming unconditional active-file protection. Clarify root mode's already-approved expiry/allocation finalization behavior.
- Both automated reproducers failed before the repair: Rust's public root test accepted the invalid ENTRY-array offset, and the shared matrix passed Go rejection cases then failed on Rust's first malformed header case. After repairing the shared validator, both writers x both SDKs reject all seven header corruptions without changing any journal bytes; valid retained histories and lazy successors still read correctly. Rust's existing array-content corruption test still permits header inventory, preserving the documented boundary.
- The affected Rust core/public SDK/log-writer suite passes again (165 public SDK, 86 core, 14 log unit, 56 existing integration and 23 root integration tests, plus applicable docs), with only the same two independently reproduced macOS group-name tests filtered. Existing compact, historical-header, snapshot and live-growth coverage passes. The changed helper's callers are fixed-header inventory, committed-arena validation for append recovery, and excluded-writer indexed snapshot capture; ordinary live mapping validation remains unchanged.
- Wiki/docs and Go runtime evidence from 664b3df remain applicable. The new checks inspect fixed header fields only and add no I/O, object traversal or per-entry probes, so prior scaling/default writer measurements remain applicable. The compatibility skill records the shared invariant and evidence-preserving parity tests. Diff and SOW audit checks pass; a focused independent recheck of the validated fix follows its local commit.
