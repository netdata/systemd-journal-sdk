# SOW-0152 - Root History Retention

## Status

Status: in-progress

Sub-state: PR #9 fixes are validated and independently reviewed in ac6ccde. Awaiting push authorization before resolving the four addressed bot threads; the CodeQL false-positive thread is already resolved.

## Requirements

### Purpose
Provide equivalent Go and Rust source-owned history retention across machine identities, with idiomatic APIs and unchanged default retention policies. Correct the default Go successor-retry cleanup gap explicitly.

### User Request
The user approved the root retention design on 2026-10-08: shared age/byte allowance, full file lengths including preallocation, whole-file tail saved-time expiration, fixed caller-selected 24-hour rotation span, no forced hourly archives, cleanup outcomes independent from healthy writes, and compact factual status. Existing consumers retain default retention policies; the final-review approval below accepts the Go successor-retry cleanup correction. The user subsequently required Go/Rust parity for this branch. The earlier Go-only assumption was not a user-approved parity waiver and is superseded. Publication remains a separately authorized operation.

### Assistant Understanding
Facts: default Log retention is machine-local, committed-byte and head-time based. Reader.OpenFile expands offsets; internal header parsing does not. Writer.archiveTo supplies existing archive durability and failure semantics.
Delivered surface: explicit strict-naming Log opt-in, standalone startup inventory, and live-aware Log inventory for runtime queries/status.
Current work: the latest GitHub Review section records the validated, independently reviewed PR9 fixes. Authorized publication and four thread resolutions remain pending; prior completed review evidence is historical. No product decision remains open. Earlier parity review found a shared Rust header-bound validation gap, fixed and independently rechecked in 3f9101c. Native Windows/Linux runtime and broader workspace validation limitations remain explicit below.

### Acceptance Criteria
- Explicit root/source inventory validates identities, filenames, state and header extents without visiting records; rejects unsafe/quarantined candidates before pruning.
- Full file lengths and tail times govern maintenance across current and retained identities; retained active finalization uses existing writer internals.
- Current live files stay protected except idle expiration or required policy transition; next file remains lazy.
- Valid policy replacement precedes maintenance; invalid policy leaves files and policy unchanged. Safe cleanup failures do not fail appends; uncertain mutation remains fatal.
- Live-aware inventory rejects missing/replaced active paths and mismatched identities before maintenance; failed writers remain inspectable for status.
- Default retention policies/file format remain unchanged; Go cleanup after a successful successor-creation retry is an intentional correction; real file fixtures, benchmark, docs/spec, audit and independent review support delivery.

## Analysis

Sources checked: go/journal/log.go, log_retention.go, writer.go, header_validation.go, current product-scope.md, project skills, pending/current SOW inventories, approved consumer design and evidence.
Pre-implementation state: root scope was absent; file metadata was available through readAppendHeader/parseHeader and arena validation. Retention errors escaped through rotation before append. The initial current SOW queue was empty; pending integration, timestamp-lane, reader and Rust performance work had no overlap with this bounded opt-in.
Delivered state: the opt-in covers owned identities, file lengths/tail saved times, lazy lifecycle/policy changes, separate cleanup outcomes, and live writer correspondence. Default retention policies and file format remain unchanged; the final-review repair documents the Go retry cleanup correction.
Risks: deleting unowned or corrupt evidence, accidental stale-policy enforcement, archive failure suppression, per-record inventory cost, and reader behavior after unlink.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:
- Existing retention intentionally owns only one machine directory and uses committed size/head time. Consumers needing an exclusive root cannot get correct aggregate maintenance by repeatedly closing a Log.

Evidence reviewed:
- log.go open/rotate/archive lifecycle; log_retention.go default policy; writer.go append-open/archive/release; native header validation; current and pending SOWs; product-scope retention contracts; approved consumer design evidence.

Affected contracts and surfaces:
- Equivalent opt-in Go and Rust Log configuration, root inventory/maintenance APIs, native header/descriptor helpers required by Rust, real-file tests, both published writer guides, product scope and the repository parity rule. No on-disk format or default retention policy change. The final-review repair explicitly corrects Go cleanup after successful successor-creation retry.

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
- SOW lifecycle: completed in done after the paired implementation and final-review repair were validated and independently reviewed.
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

Tests or equivalent validation (initial Go implementation; current paired evidence follows under Parity Correction):
- Environment: TMPDIR=/tmp; GOCACHE=/tmp/dem-sdk-go-cache; GOMODCACHE=/tmp/dem-sdk-go-mod; GOPATH=/tmp/dem-sdk-go-path; GOPROXY=off. Caches for docs are repository-local as enforced by their harness.
- `go -C go test ./...`: pass on darwin/arm64, Go 1.27.1.
- `go -C go test -race ./...`: pass on darwin/arm64, including existing default Log tests and new root retention tests.
- Go 1.26.5 `go -C go test ./journal -run TestRoot -count=1`: pass. Exact Go 1.26.2 binary unavailable; no compiler minimum changed.
- `GOOS=windows GOARCH=amd64 CGO_ENABLED=0 go -C go test -c ./journal`: pass. Native Windows runtime unlink/rename was not tested.
- `GOOS=linux GOARCH=amd64 CGO_ENABLED=0 go -C go test -c ./journal`: pass. Native Linux/stock systemd execution was not available on this macOS host; existing systemd-dependent tests skip when tooling is absent. No file-format, live publication, or reader implementation changed.
- `python3 tests/docs/check_wiki_docs.py`: all 16 wiki pages pass.
- `python3 tests/docs/verify_examples.py --lang go`: Go examples passed, including the new root-retention example. The originally recorded count of 20 was incorrect; the current harness contains 19 Go examples. Rust examples were unchanged and not rerun at this historical checkpoint.
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
- `rg -n 'enforceRetention\(|enforceRetentionOnOpen|RootRetention' go/journal/log.go go/journal/log_retention.go`: constructor, both append shapes, rotation and close share the opt-in dispatch. Default policies retain original semantics; successful creation after a failed successor attempt now enforces cleanup (the final-review section records this intentional correction). `ensureWriter` is the common fresh-allocation path for both Append and AppendRaw.
- `rg -n 'writer =|retentionWriter|rootPolicy|rootConfigured' go/journal/log.go go/journal/root_retention.go go/journal/log_retention.go`: all detach paths clear the retained readiness pointer; no pending allocation flag remains. Original caller allocation inputs feed the same normalization helper as construction.

Sensitive data gate:
- Durable changes contain synthetic identities and general contracts only. No raw sensitive data or workstation identities are recorded.

Artifact maintenance gate:
- AGENTS.md: feature parity is explicitly required during development, with user-owned exceptions and acceptance evidence in both languages.
- Runtime project skills: project-journal-compatibility documents both root opt-ins and header-native validation; release skill references the canonical parity rule.
- Specs: product-scope.md records paired ownership, accounting, lifecycle and errors.
- End-user/operator docs: Go-API.md, Rust-API.md and Writer-APIs.md document both opt-ins, verified usage and precise limits/ownership/platform caveats.
- End-user/operator skills: none exist in this repository.
- SOW lifecycle: completed in done after validation and independent review of 643afe1. This completion record is a separate commit under the current commit-before-review instructions.
- SOW-status.md: both indexes record completed status and no current SDK SOW.

Specs update:
- Product scope updated with the additive contract; defaults retained.

Project skills update:
- Compatibility skill updated so its earlier default-only retention rule does not contradict this opt-in.

End-user/operator docs update:
- Both language guides and verified examples cover root ownership, status, policy edits, error distinction and lack of exact TTL/physical cap. Shared writer guidance describes the paired contract.

End-user/operator skills update:
- No output/reference skills exist.

Lessons:
- Directory sync after unlink must receive the parent directory. Allocation transitions compare actual writer geometry with newly derived geometry; normalized buckets must not erase the distinction between defaults and caller overrides. Creation cleanup belongs to the current writer instance, and retained readiness pointers must be cleared when writer ownership ends.

Follow-up mapping:
- Consumer adoption is the separately approved coordinating task; its dependency pin awaits a separately authorized SDK release. SDK publication is outside this SOW, not deferred implementation. Native Windows/Linux runtime compatibility limitations are reported, not silently claimed. No independent implementation is bundled or deferred.

## Outcome

The original paired feature and 643afe1 repairs were validated and independently reviewed. PR9 review fixes are now implemented and locally validated; independent review and publication/thread resolution remain pending. No push has been authorized.

## Lessons Extracted

Header-native inventory has stable cost as entry count grows; recovery verification remains a separate owner responsibility. The compatibility skill and Go guide capture these boundaries.

## Followup

No independent work added. Consumer adoption is owned by the coordinating task.

## Regression Log

The 2026-10-08 live-writer inventory regression was reopened, reproduced, fixed in bf2f1e5, validated and independently reviewed. Its evidence follows.

Historical completion checkpoint (superseded by the final-review reopening): the approved SDK target was complete without a DEM compatibility path. Re-review reproduced the corrected 32-GiB-to-8-MiB transition (48-MiB old allocation to 8-MiB successor) and lazy creation cleanup (one 8-MiB active, updated maintenance time). Every verified finding is fixed; no optional review item extends this scope. Completion record is a separate commit because current user instructions require implementation commits before independent review.

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

Historical completion checkpoint before parity and final-review repairs: local SDK source was unchanged from reviewed bf2f1e5. This completion commit changes only the SOW lifecycle/current-state record and its indexes. No broad validation rerun is needed for these tracking-only edits.


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

Baseline at parity start: SDK HEAD5961564 after local-master rebase, working tree clean, root retention absent from Rust. Rust writer/default baseline tests passed with caches/output under /tmp. No other current SDK SOW overlapped; pending Rust array-open optimization and legacy-core cleanup remain independent and unactivated.

Parity gate: ready. The user request fixes the end state; there is no new product/architecture fork. Rust needs narrow fixed-header and owned-descriptor identity primitives plus archive-without-successor lifecycle, using existing dependencies. Error/result spelling and helper organization are routine language-specific implementation choices. Baseline all-feature Rust log-writer suite passes. Independent design inspection completed and its verified constraints were incorporated into the implementation.


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
- Independent design investigation found the mapped-header identity trap, original-policy normalization, fresh-file readiness and artifact-sizer builder bypass; all are implemented/tested. Independent final implementation review of 664b3df identified the fixed-header blocker and misleading rustdoc addressed below; prior whole-feature coverage remains applicable.

Parity review repair:
- Independent review of 664b3df identified a verified deletion-safety blocker: Rust's shared stable-arena validator omits ENTRY-array and tail-entry header offset bounds enforced by Go. Patching an expired archive's entry_array_offset to u64::MAX makes Go reject/preserve it, but Rust accepts inventory and deletes it during maintenance. The reproducer uses both compiled public API probes on copied synthetic fixtures.
- Repair plan: complete JournalHeader::validated_arena_end's fixed-header bounds once for retention inventory, append recovery and excluded-writer snapshot callers. Preserve ordinary live-reader mapping validation and the documented lack of object/index traversal in inventory. Add real-file rejection/evidence-preservation cases to Rust and the shared matrix; rerun affected core, public snapshot and directory writer suites. No format change or new ownership contract.
- Review also found misleading enforce_retention rustdoc claiming unconditional active-file protection. Clarify root mode's already-approved expiry/allocation finalization behavior.
- Both automated reproducers failed before the repair: Rust's public root test accepted the invalid ENTRY-array offset, and the shared matrix passed Go rejection cases then failed on Rust's first malformed header case. After repairing the shared validator, both writers x both SDKs reject all seven header corruptions without changing any journal bytes; valid retained histories and lazy successors still read correctly. Rust's existing array-content corruption test still permits header inventory, preserving the documented boundary.
- The affected Rust core/public SDK/log-writer suite passes again (165 public SDK, 86 core, 14 log unit, 56 existing integration and 23 root integration tests, plus applicable docs), with only the same two independently reproduced macOS group-name tests filtered. Existing compact, historical-header, snapshot and live-growth coverage passes. The changed helper's callers are fixed-header inventory, committed-arena validation for append recovery, and excluded-writer indexed snapshot capture; ordinary live mapping validation remains unchanged.
- Wiki/docs and Go runtime evidence from 664b3df remain applicable. The new checks inspect fixed header fields only and add no I/O, object traversal or per-entry probes, so prior scaling/default writer measurements remain applicable. The compatibility skill records the shared invariant and evidence-preserving parity tests. Diff and SOW audit checks pass; the focused independent recheck of 3f9101c found no remaining blocker.

Historical parity completion checkpoint (superseded by the final-review repair below):
- Independent read-only review of 3f9101c confirms parity with Go's relevant stable header bounds, unchanged ordinary live-reader mapping validation, appropriate stronger append/snapshot checks, corrected rustdoc and preserved rejected file bytes. It retained earlier whole-feature coverage and inspected source plus recorded failing/passing evidence without rerunning tests. No unresolved review blocker remains.
- The earlier parity assessment was contradicted by the final review of 89d8716. The following repair section is the authoritative result for lifecycle, failure and configuration behavior; the unchanged policy/inventory coverage from earlier reviews remains applicable.
- Local master 2b33553 is an ancestor of the branch after the requested rebase. This final tracking commit records completion separately because the current user instructions require validated implementation commits before independent review. SOW moves to done, both indexes record completion, and final audit/diff/status checks verify consistency. No push, publication or additional history rewrite.


## Regression - 2026-10-08 Final Review

User authorization and current target:
- The user requested fixes after a read-only accept/reject assessment of the final review of 89d8716. This authorizes the verified runtime defects, coupled tests/docs/records, and the stated recommendation to retain Go cleanup on successful creation after a safe failed successor attempt as an intentional default-mode correction.
- Preserve the approved root retention policy: full lengths, tail age, no newest-file grace, lazy successors even after eager startup finalization, complete fail-closed inventory, and caller-verified recovered active files. No compatibility adapters, release, push or unrelated work.

Verified cause and evidence:
- Rust keeps an active-file handle at its pre-rename path when directory sync fails after archive rename; standalone inventory succeeds but Log inventory fails NotFound. A fault-injected real-file test reproduces this.
- Rust successor options inherit compactness from the recovered header, while root maintenance derives desired geometry from current configuration. Reopening a regular 4-GiB configuration as compact with an 8-GiB rotation limit reproduces removal of the empty successor and an append unwrap panic.
- Go live root archives omit observer events; retired finalization emits Rotated without a successor. A retained archive after policy-change finalization produces no event.
- Both SDKs poison a healthy current writer when opening a read-only retired active fails before any mutation. Public real-file probes confirm PermissionDenied and byte-identical retired files.
- Go returned inventory aliases stored maintenance status. Rust truncates positive sub-microsecond age to zero while Go uses one microsecond. Both differences are reproduced.
- Existing reproduction logs live under .local/claude-review-triage. Shared baseline 89d8716 remains unchanged; all prior tests and review evidence that do not cover these defects remain historical evidence.

Repair gate: ready. The requested fixes and approved existing contracts fix the target; no unresolved product decision remains.

Repair plan:
1. Define lifecycle ownership and mutation boundaries explicitly. Rust archive failure must release detached active ownership while keeping failure terminal; new root successors must use the same configured options as geometry checks. Separate safe pre-mutation retired-open failures from uncertain post-mutation failures in both SDKs.
2. Emit a coherent Go archive-without-successor lifecycle event for root live/retired finalization and close. Preserve normal Rotated semantics. Isolate stored Go result ownership and align age normalization.
3. Add failing-before/fixed-after regressions for the reachable failures, strengthen recursive evidence-preservation tests, and extend shared file tests at affected lifecycle boundaries. Retain Go default cleanup-on-retry and test/document its intentional correction.
4. Clarify newest-file deletion under a too-small allowance, Close maintenance, root eager/lazy semantics and metadata-directory readability. Correct completion claims and example counts. Validate affected/full SDK suites as available, shared tests and examples, commit coherent fixes, obtain independent review, resolve verified blockers and then close the SOW.

Finding dispositions:
- P2-1/2/3: verified production/contract defects; fix.
- P3-1/3/4: clarify docs, retain approved behavior. Close retention is already documented in both general guides; clarify root-specific consequences.
- P3-2: preserve and explicitly document cleanup-on-creation after a safe failed successor attempt; it follows the existing per-created-writer rule.
- P3-5: reject the supplied zero-tail-boot fixture as a valid-input defect: VerifyIndex reports tail_entry_boot_id mismatch. Do not weaken recovery verification or invent a new boot identity for that corrupt fixture.
- P3-6/7/8: strengthen evidence-preservation assertions, copy stored results, update records.
- P3-9: fix reproduced age precision and pre-mutation poisoning defects. Dropped secondary error and duplicate startup maintenance remain nonblocking and unchanged: the first maintenance error remains visible and invalid samples remain unknown; a repeated startup sweep is bounded. Neither is needed for the ownership/allocation repairs, and no aggregate error API is added.

Risk and validation:
- The main risk is moving failure boundaries too late, permitting writes after partial mutation, or losing archive events/sequence state during successor replacement. Regression tests must exercise actual journal files, both append shapes and compact changes in both directions, failure before and after rename, healthy append after permission recovery, and preserved poisoned status after uncertain mutation.
- Source changes remain in this SDK. Synthetic identities and fixtures only; no host probes, native services or live journals. Isolated caches/output remain under /tmp or .local; no dependency/compiler changes. Existing native Windows/Linux and offline workspace limitations remain explicit.
- Current instructions require validated local commits before independent review. Read-only reviewers must not edit or launch agents. Completion tracking follows review rather than claiming the implementation reviewed before it is.

Implemented repair and validation evidence:
- Rust active ownership is detached before live archive mutation. `ActiveFile::activate_opened` marks the explicit mutation boundary after nonmutating open/validation; post-mutation errors remain fatal. Root initial/successor creation and geometry checks share configured options. Default-mode inherited-successor behavior is retained.
- Go append-open validates before writable mapping and classifies mapping/header-publication failures as uncertain. Retired open/validation failures remain safe. Root finalization/close emit Archived before pruning, stored inventories are copied, and unsafe preflight tests compare recursive file contents.
- Four Rust failures reproduced before fixes: post-rename inspection, compact-layout transitions, read-only retired open and 1-ns expiry. Permanent tests cover both layout directions, raw/structured append, sequence readback, a real PermissionDenied path, and fatal retired post-mutation failure. All-feature log-writer suite passes 24 unit, 56 existing integration, 26 root integration and 2 doc tests; evidence: .local/claude-review-triage/rust-full-after-fix.log.
- Go lifecycle, returned-status ownership, permission and append-open mutation-class regressions failed before repairs. Targeted suite passes after repairs; full `go -C go test ./...` passes. Logs: .local/retention-validation/final-review-go-before.log, final-review-go-after.log and final-review-go-full.log. Permission denial ran on this unprivileged macOS account, rather than skipping.
- Shared `run_root_retention_parity.py` passes the expanded 2-by-2 writer/reader matrix: tail/path size/count ordering, 1-ns expiry normalization, large-policy shrink with sequence-preserving lazy successor, close below the minimum allocation, archive-before-delete events, unsafe preflight and actual record readback. Evidence: .local/retention-parity/run-o6dfh7jm/report.json.
- Docs clarify Close enforcement, minimum allocation/newest-file deletion, readable metadata directories, eager startup with lazy successors, copied Go results, Archived event meaning, safe open failures and the default Go retry correction. Skill/spec record the mutation boundary and configured-successor invariants.
- `go -C go test -race ./journal` passes (32.2 seconds); Windows amd64 Go test binary compiles. Native Windows/Linux execution remains untested.
- Wiki validation passes all 16 pages; executable example validation passes all 35 examples (19 Go, 16 Rust). SOW audit and `git diff --check` pass. Logs: .local/retention-validation/final-review-go-race.log, final-review-docs.log and final-review-audit.log.
- Reference scan: `rg -n 'OpenWithOptions|newAppendWriter|recordFailure|LogLifecycleArchived|emitRootArchived' go/journal` and `rg -n 'ActiveFile::create|ActiveFile::open|activate_opened|configured_file_options|archive_root_active' rust/src/crates/journal-log-writer` confirm shared open failure classification and all root archive/creation call sites. Normal rotation remains paired; empty disposal emits no archive. No replaced path or compatibility adapter remains.
- Readiness assessment: mutation boundaries, rotation configuration and lifecycle ordering interact across both languages. Earlier final review covers unchanged feature behavior, but the repaired boundaries require independent read-only review of the committed fixes and their callers before completion. Performance-sensitive entry paths gain no inventory or filesystem work; the Go defensive slice copy is confined to maintenance/status boundaries.

Final review and completion:
- Independent read-only review of 89d8716..643afe1 found no verified blocker. Coverage included Rust post-rename ownership/inspection, pre-mutation open versus activation, configured successors, Go append-open uncertainty, lifecycle ordering, copied results, retry cleanup, close/lazy semantics, tests and docs. The reviewer inspected source and recorded evidence without rerunning tests or changing files. Prior independent coverage is retained for unchanged feature behavior.
- The successful shared matrix JSON omits four human-readable tiny-age labels that were added later; the executed assertions and all four commands are present in .local/retention-validation/final-review-parity.log at lines 30-31, 58-59, 102-103 and 130-131. The reviewer verified this evidence; no test behavior changed after that passing run.
- Final dispositions: P2-1/2/3 and P3-7 are fixed; P3-9's reproduced permission and age defects are fixed. P3-1/2/3/4/8 are resolved with truthful docs/records and approved behavior retained. P3-6 has recursive evidence assertions in both SDKs. P3-5 remains rejected as an invalid recovered fixture, not a reason to invent identity or weaken verification. The secondary resample error and duplicate startup pass remain nonblocking as assessed above.
- Re-evaluated the clean target and coupled references: both SDKs deliver the approved owned-root policy with corrected lifecycle and failure guarantees. No compatibility adapter, new dependency, unrelated implementation or in-scope partial remains. Native Windows/Linux runtime and broader offline workspace validation remain explicit limitations, not passing claims.
- Completion moves this SOW to done and updates both indexes. The implementation was committed before review; this final commit records review and lifecycle only. No push, release or history rewrite.

## Regression - 2026-10-08 GitHub Review

Authorization and target:
- User requests fixes for PR #9 bot suggestions, reply/resolution of wrong or nit-only threads, and valid Codacy findings including nits. Scope is selected GitHub bot comments and Codacy, with the mirrored CodeQL alert. No human reply, unrelated analyzer backlog, review trigger, policy weakening or push is authorized.
- PR head equals local 1abb52d, base local master 2b33553. Six inline comments (one already fixed/resolved) and fourteen Added Codacy issues were fetched completely. Codacy pagination reports total14; GitHub pages were shorter than100 and every thread comment page is complete. Evidence is ignored .local/pr9-review; no token is saved there.

Repair gate: ready. Existing approved public behavior and the request fix the target.

Root causes and dispositions:
- Empty retired-file removal increments DeletedFiles only after directory sync; a sync failure underreports actual removal. Rust has the same ordering for both live and retired empty actives. Count removals when unlink succeeds and preserve errors/unknown inventory.
- Go append-open classifies checkArenaSize failure as uncertain even though it precedes mutation, relevant to oversized files on32-bit Unix. Run the shared mapping-size preflight before the mutation boundary; retain fatal classification after mapping begins.
- Go/Rust root examples should close normally to demonstrate shutdown retention; reserve CloseWithoutRetention for the explicitly documented no-prune use case. Clarify that8MiB is new SDK allocation granularity, not an inventory acceptance or allowance floor.
- The archive-event comment is already fixed in643afe1 and resolved remotely. CodeQL3691 concerns identifiers emitted by the controlled synthetic test probe; verify and dismiss with the test-only rationale, then reply/resolve its thread.
- Codacy: seven complexity/length findings across Rust inventory/header validation and three shared probes, one Python spacing finding. Decompose coherent operations without changing guards or test assertions. Six security-audit findings cover Windows FFI, Rust test CLI arguments and Python subprocess execution; audit each exact construct and document narrow rule-specific suppression for verified safe necessary operations, following repository patterns. No global analyzer exclusion.

Plan and ownership:
1. Coordinator owns Go runtime/tests, docs/spec/SOW, remote triage and final integration. Reproduce deletion status before fixing, investigate32-bit execution availability and retain exact mutation boundaries.
2. One implementer exclusively owns rust/src/crates/journal-log-writer/** and rust/src/crates/journal-core/src/file/{header_inventory.rs,mmap.rs}: paired deletion fix, bounded inventory decomposition, audited FFI annotations and relevant tests.
3. Another implementer exclusively owns tests/interoperability/root_retention/** and tests/interoperability/run_root_retention_parity.py: behavior-preserving scenario decomposition, audited security annotations, spacing and matrix checks. No overlapping writers; agents do not commit or post reviews.
4. Run relevant Go/Rust suites, shared2x2matrix, docs, local analyzers where available and audit. Commit validated changes before any warranted independent review, retain earlier review coverage where valid, then reply/resolve each bot thread individually with evidence. Remote code must not be described as fixed before the commit is present there. If pushing is necessary, present the completed result and request authorization as the final step.

Validation and risk:
- Failure accounting must preserve successful unlink counts even on later sync failure, keep current writer healthy, and prevent pruning after uncertain writer mutation. File validation refactoring must preserve all header/identity/ownership guards and complete preflight. Test refactoring must preserve every scenario and assertion.
- Caches/temp output stay under this repository or /tmp, all fixtures synthetic. No host journal/identity probes, dependency/compiler changes or services. Existing offline Rust-workspace and native Windows/Linux execution gaps remain explicit.
- Current native-agent review instructions override the absent legacy external-reviewers harness, as recorded in prior decisions. This gate records user authorization before source or remote mutations.

GitHub repair implementation and evidence:
- Go retired empty removal and Rust live/retired empty removal now update the attempt counter at successful unlink, before directory sync. Before logs demonstrate underreported zero counts; after regressions prove one deletion, retained error, unchanged last-success, unknown inventory and healthy retry. Evidence: .local/pr9-review/go-before.log, go-after.log, rust-empty-count-before.log, rust-retired-empty-count-before.log and rust-writer-after.log.
- Go32-bit Unix size preflight was reproduced with a sparse2GiB retained active: mapping-size rejection incorrectly carried ErrWriterFailed. The same compiled regression passes after moving the existing checkArenaSize preflight ahead of the mutation boundary. Execution used the already installed Debian Docker image, no network, with386 emulation; no images/tools were installed. Files: go-386-before.log and go-386-after.log. An existing test timestamp expression overflowed int during386 compilation; a one-line uint64-before-addition correction in reader_directory_test.go is coupled test portability work.
- Rust inventory scanning and fixed-header validation are decomposed into coherent helpers while retaining all guard conditions and ordering. Shared test probes are split by scenario/build/orchestration: all17 Python assert ASTs and8 scenario bodies per Go/Rust are preserved. New allocation wording and normal-close root examples are updated in both guides/shared guide/spec.
- Full Go suite and journal race suite pass; Rust all-feature writer suite passes26unit+56existingIntegration+26rootIntegration+2docs, core default suite passes86tests (3existing docs ignored). The2x2sharedmatrix passes14checks in .local/retention-parity/run-91w6yrc9/report.json. All35docs examples and16wiki pages pass. SOW audit and diff checks pass.
- Cached Lizard validates all five affected files with zero CC20/NLOC100 warnings. Rust root inventory CC31->10 and header reader23->9; probe maxCC10/maxNLOC49. Targeted Bandit B404/B603 passes. Python E306 is directly verified by required blank-line separation; pycodestyle is unavailable. Existing B101 test assertions are intentionally retained.
- Windows FFI uses valid borrowed handles and aligned output storage; assume_init follows checked API success. The Rust CLI uses args only as test inputs, not a security decision. Python intentionally runs caller-selected Cargo and synthetic probe argv with shell=False; no journal content is executed. Narrow named audit annotations follow existing patterns. Cached Semgrep1.179.0 with the three public rule definitions and exact reported IDs reports zero findings/errors across all three targets. Native wrapper required explicit cached certificate path and pysemgrep entrypoint; no dependencies changed. Local analyzer results do not claim the hosted gate has rerun.
- CodeQL3691 was dismissed as used in tests and its thread replied/resolved. The probe writes fixed synthetic identifiers and uses Source::Unknown(history), never source_basename's User(uid) branch or host journals. One prior archive-event thread was already fixed/resolved at643afe1. Four valid new bot threads remain pending publication of the current fixes before reply/resolution.
- Same-cause search: `rg -n 'finalizeRetiredActive|discardEmptyOpenedWriter|DeletedFiles|syncJournalDirectory' go/journal` and `rg -n 'archive_root_active|finalize_retired_root_active|deleted_files|sync_empty_root_directory' rust/src/crates/journal-log-writer` covers empty/live/retired/prune paths; other removal counters already precede sync. `checkArenaSize` remains the common platform bound validation, reused before append-open's mutation boundary.
- Review readiness: the two runtime corrections plus preserved guard decomposition touch deletion/failure behavior. Obtain a bounded independent review of the committed repair and relevant callers; prior feature/Claude PASS coverage remains applicable outside this change. No product contract fork or unapproved scope expansion remains.

GitHub repair readiness checkpoint:
- Validated implementation is ac6ccde. Independent read-only review of 1abb52d..ac6ccde found no verified blocker. Coverage included successful-unlink accounting through later sync failure; poisoning, retry, close and drop paths; Go size preflight before mutation; Rust inventory/header guard preservation; probe scenarios/assertions; narrow audit annotations; and documentation. The reviewer inspected source, regression logs and the passing14-check matrix and ran git diff --check without editing files or rerunning tests. Earlier unchanged-feature review remains applicable.
- Local Semgrep before/after reproduction found the two Windows unsafe audit findings before and zero selected findings after. The Rust args and Python taint warnings did not reproduce with the cached engine and fetched rules; their annotations are justified by construct inspection, not a claimed failing-before run. Hosted Codacy validation remains pending publication.
- Final remote refresh still reports head1abb52d and the same four open bot threads. CodeQL is no longer a failed check after the test-only dismissal; Codacy remains action_required on the old head. No new bot finding or material validation gap appeared.
- The clean target and scope remain satisfied by the local implementation; only publication and the already-authorized per-thread replies/resolutions remain. Request explicit push authorization under the user's standing Git rule before publishing ac6ccde and the tracking records.
