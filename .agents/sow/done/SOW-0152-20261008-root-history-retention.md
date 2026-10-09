# SOW-0152 - Root History Retention

## Status

Status: completed

Sub-state: human review 5475128273 is addressed. Both SDKs count declared hash tables in the population budget; paired regressions, affected suites, cross-language preservation checks and SOW audit pass.

## Requirements

### Purpose

Provide equivalent Go and Rust source-owned history retention across machine identities, with idiomatic APIs and unchanged default retention policies. Correct the default Go successor-retry cleanup gap explicitly.

### User Request

The user approved the root retention design on 2026-10-08: shared age/byte allowance, full file lengths including preallocation, whole-file tail saved-time expiration, fixed caller-selected 24-hour rotation span, no forced hourly archives, cleanup outcomes independent from healthy writes, and compact factual status. Existing consumers retain default retention policies; the final-review approval below accepts the Go successor-retry cleanup correction. The user subsequently required Go/Rust parity for this branch. The earlier Go-only assumption was not a user-approved parity waiver and is superseded. Publication remains a separately authorized operation.

### Assistant Understanding

Facts: default Log retention is machine-local, committed-byte and head-time based. Reader.OpenFile expands offsets; internal header parsing does not. Writer.archiveTo supplies existing archive durability and failure semantics.
Delivered surface: explicit strict-naming Log opt-in, standalone startup inventory, and live-aware Log inventory for runtime queries/status.
Current result: prior human and cubic corrections were published through 731a37d. Human review 5475128273 identified the remaining implicit hash-table count omission; both stable validators now include those objects in the checked budget. New regressions fail before the correction and pass afterward in both languages and all four producer/consumer combinations. The separate writer-offset lead remains in pending SOW-0155. The latest user authorization includes pushing and replying; delivery is confirmed by the remote commit/comment, not by local completion alone.

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

The selected human and cubic findings are dispositioned. Prior repairs were published through 731a37d. Human review 5475128273 is accepted and repaired in both languages: the stable object budget now includes both declared hash-table objects, with historical field gates retained. Regression, race, affected Rust and cross-language evidence passes. SOW-0155 separately tracks the pre-existing writer-offset lead without implementation authorization. Push and response are authorized; hosted checks and publication must be verified on the delivered commit. No release is authorized.

## Lessons Extracted

Header-native inventory has stable cost as entry count grows; recovery verification remains a separate owner responsibility. The compatibility skill and Go guide capture these boundaries. Population invariants must account for every declared object type, including types without explicit counters; derive the model from the format and writer, not only the list of header counters.

## Followup

Consumer adoption is owned by the coordinating task. SOW-0155-20261009-writer-tail-offset-representability preserves the distinct pre-existing writer-side lead from cubic comment 4234196552; no implementation is authorized by that pending record.

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

- User requests fixes for PR #9 bot suggestions, reply/resolution of wrong or nit-only threads, and valid Codacy findings including nits. Scope is selected GitHub bot comments and Codacy, with the mirrored CodeQL alert. No human reply, unrelated analyzer backlog, review trigger or policy weakening is authorized. Push authorization was subsequently granted as recorded below.
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

Historical pre-publication readiness checkpoint:

- Validated implementation is ac6ccde. Independent read-only review of 1abb52d..ac6ccde found no verified blocker. Coverage included successful-unlink accounting through later sync failure; poisoning, retry, close and drop paths; Go size preflight before mutation; Rust inventory/header guard preservation; probe scenarios/assertions; narrow audit annotations; and documentation. The reviewer inspected source, regression logs and the passing14-check matrix and ran git diff --check without editing files or rerunning tests. Earlier unchanged-feature review remains applicable.
- Local Semgrep before/after reproduction found the two Windows unsafe audit findings before and zero selected findings after. The Rust args and Python taint warnings did not reproduce with the cached engine and fetched rules; their annotations are justified by construct inspection, not a claimed failing-before run. Hosted Codacy validation remains pending publication.
- Final remote refresh still reports head1abb52d and the same four open bot threads. CodeQL is no longer a failed check after the test-only dismissal; Codacy remains action_required on the old head. No new bot finding or material validation gap appeared.
- The clean target and scope remain satisfied by the local implementation; only publication and the already-authorized per-thread replies/resolutions remain. Request explicit push authorization under the user's standing Git rule before publishing ac6ccde and the tracking records.

Publication authorization: the user explicitly selected "Push and finish the PR review" after reviewing the completed fixes and independent-review result. This authorizes pushing the validated fixes and completion records to feat/root-history-retention and posting/resolving the four addressed bot threads. No release or history rewrite is included.

Hosted scan follow-through:

- Published validated fixes through0a39f35 and replied/resolved all four accepted threads individually. All six original threads are resolved. The current hosted scan cleared thirteen of the fourteen original Codacy findings. Its Opengrep engine reports the intentional argv sink on the argument line, while the narrow annotation was above the call; move the exact rule annotation to that reported line. The parsed Python AST is unchanged.
- Reopening the tracked SOW makes it eligible for existing Codacy scanning; the done queue is excluded by the established configuration. The current scan exposes fifty-two Markdown spacing findings in this SOW and its indexes. Correct blank lines around headings/lists and repeated blank lines; keep all prose and evidence. Do not broaden analyzer exclusions or treat moving to done as a formatting fix. These mechanical corrections need direct verification, not another runtime review.

GitHub review completion:

- Accepted and fixed: inaccurate empty-file deletion counts after directory-sync failure in Go and Rust; safe Go32-bit mapping-size rejection incorrectly poisoning the current writer; normal-close examples; and precise new-allocation wording. All four original actionable threads received individual evidence-based replies and were resolved after publication. The previously fixed archive-event thread stayed resolved.
- Rejected the CodeQL synthetic-identifier finding as test-only; dismissed alert3691 with its provenance rationale and replied/resolved its thread. No host identity is read. The fourteen original Codacy findings were addressed through coherent decomposition, Python spacing and six justified narrow audit annotations. Hosted scanning confirms those findings are cleared.
- The reopened tracked SOW exposed fifty-two Markdown spacing findings. Commit9f5ad63 corrects the spacing without hiding the current file from analysis, and the hosted scan confirms clearance. Codacy subsequently removed its twenty-five suggestion threads. Two newly arrived stale-status findings were already corrected in9f5ad63; each received a reply and resolution. The third new spacing thread was resolved by the reviewing bot. A complete GraphQL refresh confirms nine remaining threads, all resolved.
- The hosted scanner reported the intentional subprocess argv sink on its argument line. Placing the exact annotation there cleared the security audit; the resulting187-character line triggered E501. The final layout places the annotation immediately above that argument on its own135-character line. Parsed Python AST equality proves behavior unchanged; no broad suppression or analyzer configuration change is added.
- The final corrections are formatting and current-state records only. Direct diff/AST/line-length verification and SOW audit are proportionate; the earlier runtime, race,386 regression, shared matrix, examples and independent-review evidence remain valid. No further runtime review or repeated broad test run is warranted.
- Final pre-completion remote snapshot is9f5ad63: original findings cleared; Codacy only requests the now-corrected E501 line; Rust coverage/analysis and WIP remain running. Passing Go/docs/analyzer checks and a neutral CodeQL aggregate do not establish all-checks-green. No new runtime finding was reported.
- Both status indexes move this SOW to completed/done. The user-authorized final push includes the annotation wrapping and completion records. No release, history rewrite, unrelated implementation or deferred in-scope fix remains.

## Regression - 2026-10-09 Human Review 5474585093

Authorization and target:

- The user requests verification and fixes or evidence-based rejection of the selected PR9 review against32f9a76. All three reported defects are supported by source tracing. The request authorizes preserving files on rejected destructive root operations and completing the existing fixed-header contract in both languages. A human reply is not authorized; prepare evidence for the user instead.
- Reuse the existing branch/worktree and accepted caller-owned root policy. Protect changes completed between SDK calls; continue requiring exclusion during each call. No concurrent-external-change guarantee, locking, per-entry filesystem probes, new dependency, on-disk format change or default low-level creation-contract change.
- Clean end state: every root-owned active disposal/archive/rotation and lazy creation validates its owned path and destination before any mutation or ownership detachment. Safe rejection preserves all files and healthy retry where the API retains ownership; explicit close releases resources without writing after rejection. Stable-header validation rejects internal count/pair contradictions without reading objects. Existing historical field gates and ordinary live-reader mapping behavior remain intact.

Repair gate: ready. The existing contract and explicit fix request determine the end state; no unresolved product fork.

Evidence and cause:

- Go Log.close empties and archiveActive bypass the live-aware path/identity checks. Archive failure cleanup calls Writer.Close, which can write even after a safe preflight error unless changed to release-only cleanup. Go and Rust archive rename paths can overwrite occupied targets when callers bypass root maintenance preflight.
- Rust root close/maintenance already share a safe active finalization preflight, but rotate poisons/detaches before checking it. Both languages can truncate an unexpectedly restored active during lazy creation, a coupled instance of the same missing ownership precondition. Startup and retired maintenance already inventory before mutation within the caller-excluded call.
- Both shared stable-header validators bound extents but omit small in-arena tail-array count contradictions, offset/count pairs and object-population bounds. Existing shared corrupt-header tests emphasize out-of-bounds offsets and miss these logical inconsistencies.
- Upstream reference: systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced, src/libsystemd/sd-journal/journal-file.c:618-632 and665-692. Exact source fetched through the authenticated GitHub API into ignored local evidence. Applicable per-field header ends are216/224/232/240 and264. Normal live mapping validation must remain separate.

Plan and ownership:

1. Main agent owns Go root lifecycle and real-file preservation/retry tests, shared matrix, docs/spec/skill/SOW, integration and publication decisions. Add failing regressions before repair. Factor existing live path/header checks into root preflight and check occupied lazy destinations before Create. Keep default API behavior unchanged.
2. One implementer owns Rust journal-log-writer source/tests exclusively: extract existing active-finalization preflight, apply before rotation poisoning/detachment and reject occupied lazy targets; add failing-before/passing-after public-file tests for both append shapes and lifecycle cases.
3. One implementer owns Go header_validation.go and its new dedicated tests plus Rust journal-core stable-header code/tests exclusively. Add fixed-header population consistency once in each shared stable validator, with historical field gates and boundary-valid tests. No root lifecycle or shared probe edits from this implementer.
4. Extend the shared Go/Rust matrix with small logical header corruptions and evidence-preserving lifecycle scenarios where practical. Run relevant Go full/race, Rust core/writer/public snapshot suites, matrix and docs. Retain unchanged earlier evidence; assess lifecycle cost at actual transitions and confirm no new work on ordinary append paths. Commit validated implementation before independent review under current user instructions, obtain bounded review of all three repairs and coupled callers, then finish records.

Validation and artifact requirements:

- Reproduce active replacement and occupied archive targets with actual rename/copy operations completed between calls. Snapshot the full fixture root and parked originals before failure, require identical bytes/names afterward, no false lifecycle event, and successful retry after restoring ownership for non-consuming APIs. Cover empty/nonempty close, no-retention close, threshold rotation, initial lazy creation and maintenance-created lazy successor. Preserve fatal classification after mutation starts.
- Exercise each header contradiction in both implementations, including small physically in-bounds values and absent historical fields. Shared tests require rejected startup/inventory to preserve all files. Existing accepted array-content corruption remains outside header-only validation.
- Source-reference scans cover root create/archive/disposal call sites and all stable-validator callers. Document boundary preconditions and header consistency in paired API/shared docs, product scope and compatibility guidance. No unrelated pending SOW overlaps this ownership/validation repair.
- Synthetic identities only; no host journals/identity probes, services, new packages or compiler changes. Existing isolated Go/Rust caches and outputs stay under /tmp or ignored .local. Native Windows runtime remains untested. Current native review instructions supersede the unavailable historical external-reviewers harness; reviewers remain read-only and cannot launch agents.

Implementation and reproduction:

- Accepted all three human findings. Go root close/disposal/rotation now share path/held-file and header-identity preflight; rejected close uses release-only cleanup. Rust rotation reuses the existing safe root-finalization check before poisoning or detaching ownership. Both reject occupied archive destinations and unexpected lazy active paths before creation. Ordinary nonrotating append paths and low-level creation semantics remain unchanged.
- The shared stable validators enforce object capacity, entry/category population bounds, first/cached tail-array ordering, offset/count pairing and cached count versus entries. Root-specific duplicate population checks were removed. Present-field gates preserve historical headers; ordinary live-reader mapping validation remains separate.
- Failing-before evidence: go-boundary-before.log, rust-lifecycle-before.log and go-header-before.log under .local/pr9-human-review-5474585093. The expanded matrix against pre-fix probe binaries failed at the first new cached-offset contradiction because inventory accepted it; parity-before.log records the failure. Passing-after evidence is in corresponding after/final logs and the rebuilt matrix report.
- Adjacent real defect: an eager Rust root with size limit 1, followed by setting the default root policy, rotates its metadata-only file on first append. rust-empty-writer-before.log reproduces an empty archived file rejected by inventory. Root rotation now waits for the first entry; both append shapes retain one valid active after the first write and rotate normally on the second. Go already guards empty size/duration rotation. The earlier exploratory rust-empty-rotation-before.log is not reproduction evidence.
- Real-file regressions cover replacements, collisions, close variants, count/duration rotation and initial/post-maintenance lazy creation. They compare names and bytes, include parked originals, require no false lifecycle event, and check healthy retry after path restoration. Header tests cover regular/compact fixtures, small contradictions, historical absence and equality bounds. The shared matrix proves rejected inventory/startup preserve bytes across both writers and both readers.

Validation of the repair:

- Cache isolation uses the existing /tmp/dem-sdk-go-cache, /tmp/dem-sdk-go-mod, /tmp/dem-sdk-go-path, /tmp/dem-sdk-cargo and /tmp/dem-sdk-rustup; output targets are under /tmp or ignored .local. Go runs with GOPROXY=off; Rust runs with CARGO_NET_OFFLINE=true. No dependencies, compiler versions, services or host journals changed.
- `go -C go test ./...` and `go -C go test -race ./...`: pass, including all new lifecycle/header regressions and existing default Log, snapshot and recovery tests. Logs: go-full.log and go-race.log.
- Rust journal-core suite: 89 tests pass, three existing doc tests ignored. `cargo test --manifest-path rust/Cargo.toml -p systemd-journal-sdk-log-writer --all-features`: 26 unit, 56 existing integration, 26 root-retention integration, six lifecycle test groups and two doc tests pass. Logs: rust-header-after.log and rust-writer-all-features.log.
- Rust public SDK suite: 165 tests pass, including concurrent append/snapshot and historical-header cases. Two unchanged macOS group-resolution tests fail because group ID zero resolves to wheel rather than root: plugin_compatible_profile_caches_user_group_resolution and plugin_compatible_profile_resolves_user_group_ids_explicitly. Repeating with exactly those two tests excluded passes. Logs: rust-public.log and rust-public-supported.log. Native Linux/Windows runtime and the previously recorded broader offline workspace gaps remain untested.
- `python3 tests/interoperability/run_root_retention_parity.py --cargo /tmp/dem-sdk-cargo/bin/cargo --benchmark`: 14 scenario groups pass with expanded header damage coverage; report .local/retention-parity/run-1107m1ea/report.json. Header-only inventory averages 33.7-37.3 microseconds across Go/Rust writers/readers for one versus 10,000 entries, 1,000 iterations each. Rust is a debug build; these local samples support record-count-independent work, not a throughput or speedup claim. New lifecycle checks add fixed metadata/header reads only at transitions.
- Wiki validation passes all 16 pages; verified examples pass 35/35. No example code changed. Lizard checks of changed helpers/tests and the shared matrix show no new configured complexity/length violations. A broader scan misparses the unchanged Rust LogArtifactSizer trait declaration as a long method; the same warning reproduces on baseline 32f9a76 and is not a source defect. `git diff --check` passes. SOW audit and independent review are recorded at the final checkpoint below.

Reference search and clean-target assessment:

- `rg -n 'preflightRootActive|discardEmptyOpenedWriter|archiveActive\(|archiveTo\(|Create\(|os.Remove\(|os.Rename\(' go/journal/log.go go/journal/root_retention.go`: live root boundaries use common preflight; retired/startup operations use the complete inventory within the caller-excluded call. Default-chain repair/disposal and low-level Create/ArchiveTo retain their established contracts. No bypass remains for changes completed between root SDK calls.
- `rg -n 'preflight_root|archive_root_active|archive\(|rotate\(|remove_file\(|rename\(' rust/src/crates/journal-log-writer/src/log`: root creation/rotation and close/drop/maintenance share the boundary checks. Retired/startup mutation remains preceded by complete inventory. Default chain and low-level writer paths are intentionally unchanged.
- `rg -n 'validateDeclaredArena|validated_arena_end|validate_file_size' go/journal rust/src/crates/journal-core/src rust/src/journal/src`: stronger fixed-header validation covers root inventory, append recovery, excluded-writer snapshots and existing stable verification; live mappings remain separate. Coupled tests/docs/spec/compatibility guidance are updated. No deferred implementation or unrelated pending work is included.
- Readiness assessment: these ownership/error boundaries and shared header callers warrant one fresh independent reviewer who did not implement them. Review the coherent repair and relevant callers, retaining earlier whole-feature evidence where unchanged. Current instructions require a validated implementation commit before that read-only review, with completion tracking in a follow-up commit.

Final review and completion checkpoint:

- Fresh independent Astra/high reviewer root_retention_human_fix_review examined 32f9a76..df8c215 and relevant callers, without editing code or launching agents. No blocker found. Coverage included lifecycle ordering/ownership, safe retry versus fatal mutation, default-mode preservation, historical header gates, separate live mappings, transition-only filesystem cost, docs and real-file regression evidence.
- The reviewer independently reran the focused Go lifecycle/header cases, six Rust lifecycle groups and three Rust header tests; all passed. Full suites, native platform tests and the shared matrix were not independently repeated; main-agent evidence and its limitations remain recorded above.
- The reviewer caught an evidence-command typo: the writer package is systemd-journal-sdk-log-writer, not journal-log-writer. The validation record is corrected; the actual earlier log already showed the intended package tested. This tracking-only correction does not invalidate runtime validation or review.
- No verified blocker, deferred repair or new independent implementation remains. Paired docs/spec/skill changes capture the delivered contract outside this SOW. Status moves to completed/done and both indexes are updated. SOW audit reports initialization complete and clean; final diff checks are clean. No source changes follow the reviewed implementation commit. At that local completion checkpoint no remote action had occurred. The subsequent authorized push reached 993279f and the human response was posted at https://github.com/netdata/systemd-journal-sdk/pull/9#issuecomment-6088470864. No release occurred.

## Regression - 2026-10-09 Cubic Review 5474943738

Repair gate: ready. The verified aggregate-count and documentation corrections are complete. The large-offset validator suggestion is rejected after the format investigation recorded below.

Authorization and target:

- The user requests verification and fixes or evidence-based rejection of cubic findings on PR #9. Earlier authorization to push and reply on this PR remains in effect for the review follow-up. No merge, release or review dismissal is authorized.
- All seven selected comments and their seven unresolved threads were fetched completely, with no further pages. Baseline is clean 993279f; hosted checks pass or are intentionally skipped. No unrelated current SOW overlaps this repair.
- Clean target: stable header validation rejects impossible totals of disjoint present object categories in both SDKs; absent historical fields remain ignored and ordinary live mappings unchanged. Consumer docs promise retry only for preflight rejection. Publication records report verified delivery without describing a published commit as local. Determine the format-valid behavior for the alleged wrapped cached offset before changing any relevant contract.

Evidence and root cause:

- Go writer_objects.go/writer_init.go/writer_arrays.go and Rust writer.rs/writer_entry_arrays.rs increment individual counters for disjoint object types, while the total counts all objects. The stable validator only compares each type against the total, allowing combined counts greater than that total. Boundary tests accidentally endorse this impossible combination.
- Go ensureWriter sends Create errors to recordFailure; its preflight path returns directly before creation. The Go guide says any rejected creation/rotation remains retryable, while the shared guide already specifies preflight. Narrow the paired spec/wording consistently.
- Prior publication was verified in ignored delivery evidence but current SOW/index summaries were not updated. Replace those stale summaries with the known published commits and posted-response link. Preserve earlier timing evidence as historical.
- Cubic's claimed zero-hint fallback conflicts with the pinned fixed-header consistency rule. A bounded independent read-only investigation checks format and writer reachability; a cast alone does not justify accepting corrupt files.

Plan, validation and ownership:

1. Main agent owns the SOW, aggregate validators and paired tests/shared matrix, docs/spec/skill updates. First reproduce aggregate rejection failure in both languages and the real-file shared matrix. Use a remaining-object budget so accumulation cannot overflow; replace impossible acceptance fixtures with valid per-counter and aggregate boundaries.
2. One read-only Astra/high investigator owns assessment of the large-offset claim, source pointers and any reproducer needed. No implementation overlap, source edits or spawned agents. Resolve evidence before dependent implementation; escalate only a genuine product/format fork.
3. Validate affected Go/Rust core/writer/snapshot behavior, the expanded shared matrix and required docs; retain earlier lifecycle/race evidence where unchanged. Document limitations. Commit validated changes before any independent review required by the final risk assessment; reuse existing reviewed contracts where their assumptions hold.
4. Record a disposition per comment, complete SOW/audit checks, refresh selected findings/CI before authorized push, and reply to/resolve selected bot threads individually with specific evidence. No unsupported claim that post-push CI or all PR review sources are complete.

Risk, scope and artifacts:

- Header inventory is fixed-size consistency validation, not full object/index verification. Present population categories must fit within the total. This checkpoint omitted implicit hash-table counts; review 5475128273 below corrects that incomplete model. Unreported historical or future categories may still leave an unused budget. No record scanning, new dependencies or per-append filesystem work. The arena/total check remains before subtraction.
- Changes use synthetic journals and existing offline caches/output under /tmp or ignored .local. No host journals, identity probes or services. Native Linux/Windows execution gaps remain explicit. Updated acceptance evidence belongs in tests, docs/spec/compatibility guidance and this tracked SDK SOW.

Finding dispositions and scope:

- Accepted 4234196577: disjoint object counts can each be within n_objects while their aggregate exceeds it. Both validators now consume a remaining-object budget after proving n_entries <= n_objects; absent historical fields are skipped. Corrected boundary fixtures test an aggregate exactly at the bound instead of assigning the total to every category.
- Accepted 4234196520: Go retry prose now names preflight rejection, and the paired spec is equally precise. Errors after mutation remain under the existing terminal-failure contract. The shared/Rust guides already name preflight and need no change.
- Accepted 4234196582,4234196593,4234196603,4234196608: the human fixes were published through 993279f and replied to at issuecomment-6088470864. Updated current/Outcome/index summaries and qualified the earlier pre-publication checkpoint as historical. Completion records avoid volatile unpublished/no-reply claims without a dated checkpoint.
- Rejected 4234196552's validator relaxation: systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced, src/libsystemd/sd-journal/journal-file.c:618-628 explicitly rejects first-array > cached tail and zero-offset/nonzero-count disagreement. Rust writer_entry_arrays.rs:152-166 and Go writer_arrays.go:69-85 scanning when the hint is zero does not make wrapped metadata valid; most oversized offsets wrap to nonzero wrong locations instead. Main-agent tracing and independent Astra/high source review agree. No runtime reproduction was claimed for this separate writer concern.
- The unchecked regular-writer casts are pre-existing, in unchanged producer code, and require an allocation/publication and failure-contract decision rather than a validator exception. Recorded in pending SOW-0155-20261009-writer-tail-offset-representability with exact source evidence, reproduction requirements and explicit lack of implementation approval. The current validation fix is complete without permitting invalid headers; this lead is not relabeled as a valid fallback.

Repair validation and readiness:

- Both new aggregate-counter regression suites failed before implementation: Go accepted all five over-budget categories in regular and compact fixtures; Rust accepted the first aggregate case; historical present-counter gates failed in both. The unchanged probe binaries also failed at the new real-file aggregate-DATA case. Logs are under .local/pr9-cubic-5474943738: go-before.log, rust-before.log and parity-before.log.
- After the shared remaining-budget correction, `go -C go test -race ./...` passes. Rust default core passes 89 tests with three existing doc tests ignored. `cargo test --manifest-path rust/Cargo.toml -p systemd-journal-sdk-log-writer --all-features` passes 26 unit, 56 prior integration, 26 root integration, six lifecycle groups and two docs. The public SDK suite passes 165 tests, excluding the same two previously reproduced macOS root/wheel group-name assumptions. Logs: go-race.log, rust-core.log, rust-writer.log and rust-public.log.
- An attempted combined core/writer all-features invocation stopped on uncached allocative 0.3.6 in offline mode. No dependency was installed. This is the previously recorded optional all-feature workspace gap; the actual changed core behavior is exercised by its default suite, all-feature writer, public SDK and paired probes. Native Linux/Windows execution remains untested locally.
- Rebuilt Go/Rust parity matrix passes all 14 scenario groups, including the new aggregate corruption and unchanged-byte startup/inventory assertions for both producers/consumers. Report: .local/retention-parity/run-4dfkm_ok/report.json. Wiki validator passes 16 pages; all 35 compiled/executed examples pass. Earlier actual lifecycle byte-preservation regressions remain unchanged and pass within these suites.
- Source scan `rg -n 'validateHeaderPopulation|validate_header_population|validateDeclaredArena|validated_arena_end'` over the changed validators and their writer/root/snapshot callers confirms the correction is shared. Ordinary live mapping validation is untouched. The change replaces four independent comparisons with four checked subtractions; no additional I/O, allocations, record-dependent loop or benchmark claim is introduced.
- Readiness assessment: direct verification is sufficient for the bounded aggregate-budget correction and prose/status changes. The invariant is disjoint object counts, subtraction follows a checked initial bound, and every present-field subtraction is guarded; real-file parity and broad affected suites check integration. Cubic supplied independent challenge of this invariant; prior human/independent lifecycle review remains applicable. The separate format question received independent read-only source investigation and main-agent verification. No further material uncertainty remains in this repair that calls for another review round.
- Compatibility guidance and product scope record the aggregate rule, and Go prose matches the already-correct shared/Rust preflight contract. Six accepted thread IDs and the rejected suggestion are listed above. SOW moves back to done with both indexes updated and SOW-0155 pending. Lizard on changed helpers/tests/shared harness shows no configured warnings; audit reports initialization complete and clean, and diff checks pass.

## Regression - 2026-10-09 Human Review 5475128273

Authorization and target:

- The user explicitly requested fixing, pushing and answering this review. Restore
  the approved rejection-before-pruning contract in both languages; no format or
  public API change. Earlier lifecycle review and validation remain applicable.
- Gate status: ready. This regression section supplements the original gate.

Problem / root-cause model and evidence:

- The aggregate budget enumerated five explicit counters but omitted the two
  implicit hash-table object counts. Both are distinct object types, counted by
  the writers and upstream systemd. Earlier boundary fixtures repeated the same
  incomplete model by assigning the whole budget to those five categories.
- At 731a37d, scratch public-API probes created one-row archives with six objects,
  increased only n_data from one to two, and demonstrated inventory acceptance
  followed by archive deletion at one-byte startup retention in all four Go/Rust
  producer/consumer combinations. Evidence: ignored
  .local/pr9-human-review-5475128273/report.json.
- Upstream: systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced,
  docs/JOURNAL_FILE_FORMAT.md:578-584 and
  src/libsystemd/sd-journal/journal-file.c:1262-1268.

Affected surfaces and existing patterns:

- Shared stable-header population validators, paired header tests, the existing
  real-file inventory/startup preservation matrix, compatibility skill and spec.
  Reuse the checked remaining-object budget and historical complete-field gates.
  Live-reader mapping validation, record/index traversal, retention policy and
  the separately tracked writer-offset investigation remain unchanged.

Risk and sensitive data handling:

- Incorrect table presence or historical gates could reject valid old headers.
  Count each declared table once, validate its extent using existing checks, and
  cover absent declarations and historical explicit counters. Constant-size
  header arithmetic only; no new allocations, dependencies or I/O.
- Synthetic journals only; generated evidence and caches stay in .local or /tmp.
  No host journals, identity probes, secrets or personal data in durable records.

Implementation and validation plan:

1. Add paired unit regressions and a real-file n_data-plus-one matrix case;
   demonstrate failure before runtime edits. Correct the positive budget fixture.
2. Include both declared table objects in the common checked budget. Verify
   exact and insufficient budgets for table presence and historical headers.
3. Run affected Go race, Rust core/writer suites and rebuilt cross-language
   retention matrix. Inspect same-failure callers, diffs and SOW audit.
4. Assess independent review need against the bounded change and prior evidence;
   complete coherent tracked SDK records, commit, push and reply with evidence.

Artifact impact and decisions:

- Update compatibility guidance and product scope to explicitly include implicit
  object types. Public guide contracts are already correct; examples are
  unchanged. No AGENTS, public API, dependency or output-skill changes required.
- Reopen this original SDK SOW and both indexes, then complete them together with
  the fix. No new follow-up is needed for the accepted finding. SOW-0155 remains
  an independently tracked, unauthorized investigation.
- No open user-owned decision; correction preserves the approved contract.

Repair result, validation and readiness:

- Both stable validators consume one budget item per declared hash-table offset
  using the same checked subtraction as the explicit counters. Extent validation
  remains separate and still rejects incomplete offset/size pairs. All seven
  current object types are accounted for; missing historical category fields are
  skipped, and no equality/full-graph verification requirement is introduced.
- Before runtime edits, paired header tests failed for the n_data-plus-one case
  and insufficient budgets with one/two tables in regular/compact and oldest/
  current headers. The real-file parity matrix also failed with accepted unsafe
  inventory. Logs: .local/pr9-human-review-5475128273/{go,rust,parity}-before.log.
- After correction, Go `go test -race ./...` passes. Rust core passes 90 tests
  (three existing documentation tests ignored); writer all-features passes
  26 unit, 56 existing integration, 26 root integration, six lifecycle and two
  documentation tests. Public SDK passes 165 tests, with the same two known
  macOS root/wheel name-assumption tests explicitly excluded as prior validation.
  Logs: go-race.log, rust-core.log, rust-writer.log and rust-public.log in the same
  evidence directory. No native Linux/Windows runtime claim is made; the prior
  offline optional all-features-core dependency gap remains unchanged.
- Rebuilt Go/Rust parity matrix passes all 14 scenario groups. The new real-file
  case exercises both inventories and startup at a one-byte allowance, retaining
  original journal bytes across all four producer/consumer combinations. Report:
  .local/retention-parity/run-rjeqhyee/report.json.
- Same-failure search: `rg -n
  'validateHeaderPopulation|validate_header_population|validateDeclaredArena|validated_arena_end'
  across go/journal, Rust core and log-writer confirms shared use by inventory,
  append recovery and snapshots, plus Go strict verification. Ordinary live
  mapping validation is unchanged. Compared the budget against Go verifyObject
  and Rust ObjectType: ENTRY, DATA, FIELD, TAG, ENTRY_ARRAY, DATA_HASH_TABLE and
  FIELD_HASH_TABLE are all covered. No remaining replacement/reference migration.
- Readiness assessment: direct verification suffices for this bounded correction.
  The human review independently identifies the missing object types and supplies
  the authoritative format/model; the implementation adds them to the existing
  checked budget. Main-agent review verified both declarations, all seven current
  types, complete-field gates, overflow bounds and unchanged live-reader rules.
  Paired failing-before/passing-after tests and actual filesystem preservation
  cover the reachable consequence. Earlier independent lifecycle review remains
  applicable; this correction changes no ownership/mutation sequence. No material
  uncertainty remains requiring another independent review round.
- Compatibility skill and product spec now state the implicit count invariant.
  Public guides/examples, AGENTS and dependencies are unchanged because their
  contracts already require safe rejection. No output skills exist. Sensitive
  data gate: synthetic fixtures, sanitized summaries and ignored local evidence
  only. No new deferral: the selected finding is fully fixed; SOW-0155 remains
  independent. The SOW audit reports initialization complete and clean, and
  whitespace/diff checks pass. This SOW and both indexes return to completed/done
  in the same commit as the validated repair.
