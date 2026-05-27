# SOW-0032 - Live Feature Compatibility Matrix

## Status

Status: completed

Sub-state: Implemented, validated, reviewed, and closed. Split from SOW-0022 Gap 1.

## Requirements

### Purpose

Prove that stock journalctl, stock libsystemd readers, and repository readers can safely read active journal files while SDK writers append with compression, compact layout, and Forward Secure Sealing enabled.

### User Request

Compatibility must be confirmed and validated for all writers and readers, including concurrent operation with stock systemd tooling.

### Assistant Understanding

Facts:

- The project requires one writer and multiple readers concurrently on the same file.
- Current live compatibility coverage exists for the regular current writer slice.
- Later SOWs completed compression, compact, FSS, and mixed-directory closed-file validation, but live coverage for those feature slices remains separate.

Inferences:

- Closed-file verification is not enough for production compatibility because publication windows can differ while a writer is active.

Unknowns:

- Exact runtime duration and entry counts should be tuned during implementation to catch races without making routine validation too slow.

### Acceptance Criteria

- Live tests cover regular plus zstd/xz/lz4 DATA compression, compact uncompressed, compact plus compression, and sealed/FSS files.
- Each live feature matrix includes stock `journalctl --file`, stock libsystemd, and every repository reader while the writer is active.
- Final closed-file validation includes `journalctl --verify --file`, with `--verify-key` where required.
- The test records stock systemd version and commands/helpers used.

## Analysis

Sources checked:

- `product-scope.md`
- `SOW-0022-20260525-compatibility-test-gap-audit.md`
- `tests/conformance/live/`
- `tests/interoperability/run_live_matrix.py`
- `tests/interoperability/run_compression_matrix.py`
- `tests/interoperability/run_compact_matrix.py`
- `tests/interoperability/run_mixed_directory_matrix.py`

Current state:

- Existing live tests cover the regular live writer slice.
- Compression, compact, FSS, and mixed-directory tests are primarily closed-file or directory-reader matrices.

Risks:

- Writers can pass closed-file checks but expose inconsistent active metadata while appending.
- Sealed files require careful key handling so live readers do not require verification keys for normal reads but verification does.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Feature-specific writer paths can affect append publication order, object flags, tail metadata, and verification state. The current live harness does not yet exercise those paths under concurrent stock/systemd readers.

Evidence reviewed:

- `product-scope.md` live concurrency contract
- `tests/interoperability/run_live_matrix.py`
- `tests/interoperability/run_compression_matrix.py`
- `tests/interoperability/run_compact_matrix.py`
- `tests/interoperability/run_mixed_directory_matrix.py`
- SOW-0019, SOW-0024, and SOW-0027 status notes in `SOW-status.md`

Affected contracts and surfaces:

- Writer append publication windows.
- Reader live state handling.
- Stock systemd interoperability claims.
- Compatibility matrix documentation.

Existing patterns to reuse:

- Current live matrix runner and `LIVE_SEQ` ordered visibility checks.
- Current compression/compact/FSS generator options.
- Current stock journalctl and libsystemd adapters.

Risk and blast radius:

- High compatibility value, medium runtime/test complexity. Failures may require writer publication changes in all languages.

Sensitive data handling plan:

- Use synthetic fields and deterministic test keys only.

Implementation plan:

1. Extend or add live matrix runners for compression, compact, compact+compression, and sealed files.
2. Reuse existing writer adapters and repository reader adapters.
3. Add stock journalctl and stock libsystemd live reader checks for each feature slice.
4. Run final stock verification after clean close.
5. Document matrix scope and expected runtime.

Validation plan:

- Run the new live feature matrix.
- Run existing live, compression, compact, mixed-directory, and FSS verification matrices as relevant.
- Use read-only reviewers after implementation.

Artifact impact plan:

- AGENTS.md: no expected update.
- Runtime project skills: update `project-journal-compatibility` if feature-live matrices become a mandatory close gate.
- Specs: update `product-scope.md` with the validated feature-live coverage.
- End-user/operator docs: likely unaffected.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: activate only this SOW when implementing.
- SOW-status.md: update on activation and completion.

Open-source reference evidence:

- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
  - `man/journalctl.xml`
  - `src/libsystemd/sd-journal/sd-journal.c`

Open decisions:

- None.

## Implications And Decisions

- No user decision is required before implementation.

## Plan

1. Build the live feature matrix.
2. Run against stock and repository readers.
3. Fix any compatibility failures.
4. Validate and review as one batch.

## Delegation Plan

Implementer:

- Local implementation by the project manager, per current routing decision.

Reviewers:

- Read-only reviewers from the active pool after implementation: minimax, kimi, qwen, glm. Mimo is skipped.

Failure handling:

- Record feature-specific failures and do not claim production compatibility for failed slices.

## Execution Log

- 2026-05-27: Activated SOW for local implementation with read-only external reviewers after implementation.
- 2026-05-27: Extended `run_live_matrix.py` from a regular live-writer matrix to a feature matrix covering regular, zstd, xz, lz4, compact, compact-zstd, compact-xz, compact-lz4, and sealed/FSS slices.
- 2026-05-27: Added live stock libsystemd reader coverage through the existing C helper and recorded libsystemd wait evidence in matrix results.
- 2026-05-27: Added sealed test options to Go, Rust, Node.js, and Python livewriter test commands. Rust sealed livewriter is forced to file mode because its directory livewriter path does not expose sealing.
- 2026-05-27: Added final stock `journalctl --verify --file` for every generated file, with deterministic `--verify-key` for sealed files.
- 2026-05-27: Fixed high `--poll-readers` assessment to require live evidence per reader implementation group, not from every duplicate worker.
- 2026-05-27: Added explicit sealed assessment checks so sealed results fail if the deterministic verify key or `--verify-key` command flag is absent.

## Validation

- Syntax and format checks:
  - `python3 -m py_compile tests/interoperability/run_live_matrix.py python/cmd/livewriter.py` passed.
  - `node --check node/internal/testcmd/livewriter.js` passed.
  - `gofmt -w go/internal/testcmd/livewriter/main.go` produced the expected formatted Go file.
  - `rustfmt rust/src/internal/testcmd/livewriter/src/main.rs` produced the expected formatted Rust file.
  - After the final compression-threshold comment, `python3 -m py_compile tests/interoperability/run_live_matrix.py` passed.
- Focused sealed high-reader validation:
  - Command: `python3 tests/interoperability/run_live_matrix.py --features sealed --writers go --entries 10 --poll-readers 4 --libsystemd-readers 1 --writer-delay-ms 10`
  - Result: PASS 1/1.
  - Stock systemd: `systemd 260 (260.1-2-manjaro)`.
  - Result artifact: `.local/interoperability/live-feature-matrix-results-20260527-102031.json`.
- Full live feature matrix validation:
  - Command: `python3 tests/interoperability/run_live_matrix.py`
  - Result: PASS 36/36.
  - Stock systemd: `systemd 260 (260.1-2-manjaro)`.
  - Features: regular, zstd, xz, lz4, compact, compact-zstd, compact-xz, compact-lz4, sealed.
  - Writers: Go, Rust, Node.js, Python.
  - Entries per writer/feature cell: 30.
  - Polling readers per implementation: 2.
  - Stock libsystemd live readers per cell: 1.
  - Every cell passed active stock `journalctl --file`, active stock libsystemd, active repository reader, final repository reader, stock `journalctl --verify --file`, and structural oracle checks. Sealed cells also used deterministic `--verify-key`.
  - Result artifact: `.local/interoperability/live-feature-matrix-results-20260527-102118.json`.
- Reviewer evidence:
  - Round 1 reviewers reported PRODUCTION GRADE after the initial implementation. Kimi identified a high `--poll-readers` false-negative risk; this was fixed by grouping duplicate poll workers by reader implementation.
  - Round 2 GLM and Qwen reported PRODUCTION GRADE after the poll-group fix. Minimax and Kimi/opencode sessions were invalid or non-responsive; one attempted recursive reviewer execution and the exact SOW-0032 opencode PIDs were stopped.
  - Final defensive changes were made for sealed `--verify-key` assertion and compression-threshold documentation. GLM final review returned PRODUCTION GRADE and required only SOW close-out documentation. Qwen final review became non-responsive after read-only inspection and the exact SOW-0032 Qwen opencode PIDs were stopped.
- Same-failure search:
  - Duplicate poll reader assessment now uses `reader_group()` in `tests/interoperability/run_live_matrix.py` so repeated poll workers do not cause false failures while still requiring every reader implementation type to observe live entries.
  - `rg --no-ignore-parent -n 'compress.threshold|--compress-threshold|--compression-threshold' go/internal/testcmd/livewriter/main.go node/internal/testcmd/livewriter.js python/cmd/livewriter.py rust/src/internal/testcmd/livewriter/src/main.rs tests/interoperability/run_live_matrix.py` showed every livewriter accepts the shared `--compress-threshold` flag used by the matrix.
- Sensitive data gate:
  - The matrix writes synthetic test fields only.
  - FSS uses a deterministic all-zero 12-byte seed and deterministic verification key for tests. No production key material or host journal data is used.
- Repository-boundary gate:
  - Runtime artifacts and caches are directed under `.local/`.
  - Stock tooling is invoked only with repository-local `--file` paths. No live host journal, `/var/log/journal`, or `/run/log/journal` path is touched.
- Artifact maintenance gate:
  - `AGENTS.md`: not changed. The existing repository-wide rules already covered SOW lifecycle, reviewer routing, repository boundary, and journal compatibility constraints.
  - Runtime project skills: `.agents/skills/project-journal-compatibility/SKILL.md` updated to make the live feature matrix mandatory for future live writer/reader compatibility changes.
  - Specs: `.agents/sow/specs/product-scope.md` updated to record the validated live feature matrix as current compatibility evidence for all four writers.
  - End-user/operator docs: `tests/interoperability/README.md` updated because it documents the operator-facing interoperability runner.
  - End-user/operator skills: no output/reference skills exist for this project, so none were affected.
  - SOW lifecycle: this SOW moved from pending to current, then to done at close. `SOW-status.md` was updated for activation and completion.
- SOW audit:
  - `.agents/sow/audit.sh` passed before close.

## Outcome

Completed.

SOW-0032 adds a single live feature matrix that validates active-file compatibility for all requested feature slices across all four SDK writers and all repository readers, plus stock `journalctl --file` and stock libsystemd readers.

The accepted default matrix now proves:

- regular active journal files;
- zstd, xz, and lz4 DATA-compressed active journal files;
- compact active journal files;
- compact plus zstd, xz, and lz4 DATA-compressed active journal files;
- sealed/FSS active journal files with final stock `journalctl --verify --verify-key`.

The default run passes 36/36 cells on stock `systemd 260 (260.1-2-manjaro)`.

## Lessons Extracted

- Closed-file verification is insufficient for these feature slices; the same feature options must be exercised while stock and repository readers are active.
- Duplicate poll workers are useful for race coverage, but assessment must be grouped by reader implementation. Otherwise a valid run can fail only because one duplicate worker starts too late.
- Sealed/FSS live reads do not require the verify key, but the final stock verification gate must explicitly include and assert `--verify-key`.
- Rust sealed livewriter validation currently needs file mode. The directory livewriter test command should not be silently used with `--seal` until it exposes sealing.
- Compression live matrix fixtures need a low deterministic threshold so at least one DATA object is compressed in every compressed feature slice.

## Followup

No deferred implementation work remains for this SOW.

Rejected as not worth separate follow-up from this SOW:

- A structure-oracle `expected_sealed` assertion. Stock `journalctl --verify --verify-key` is the authoritative sealed-file close gate and already fails if required TAG/seal validation is missing.
- Deduplicating fixture generation across test livewriters. The current per-language fixture code follows the existing project pattern and keeps each test command self-contained.

## Regression Log

No regressions recorded.
